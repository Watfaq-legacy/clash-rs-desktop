//! Turn share links (`ss://`, `trojan://`, `vmess://`, `vless://`,
//! `hysteria2://`) — pasted by self-hosters, or served base64-encoded by
//! providers without Clash output — into a ready-to-run Clash config.

use base64::Engine as _;
use serde_yaml::{Mapping, Value};

/// Main selector group name in configs we generate.
pub const MAIN_GROUP: &str = "节点选择";
pub const AUTO_GROUP: &str = "自动选择";

/// Parse text that is either a list of share links or base64 of one.
/// Returns `None` if no supported link is found.
pub fn config_from_links(text: &str) -> Option<Value> {
    let text = text.trim();
    let decoded;
    let body = if text.contains("://") {
        text
    } else {
        decoded = decode_base64(text)?;
        decoded.as_str()
    };

    let mut proxies: Vec<Value> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(mut proxy) = parse_link(line) else {
            continue;
        };
        let name = unique_name(&names, proxy_name(&proxy));
        proxy.insert("name".into(), name.clone().into());
        names.push(name);
        proxies.push(Value::Mapping(proxy));
    }
    (!proxies.is_empty()).then(|| build_config(proxies, &names))
}

/// Whether the text looks like share links rather than a URL or YAML.
pub fn looks_like_links(text: &str) -> bool {
    let text = text.trim_start();
    [
        "ss://",
        "trojan://",
        "vmess://",
        "vless://",
        "hysteria2://",
        "hy2://",
    ]
    .iter()
    .any(|scheme| text.starts_with(scheme))
}

fn build_config(proxies: Vec<Value>, names: &[String]) -> Value {
    let list = |items: Vec<String>| Value::Sequence(items.into_iter().map(Value::from).collect());
    let mut select = vec![AUTO_GROUP.to_owned()];
    select.extend(names.iter().cloned());

    let groups = vec![
        map([
            ("name", MAIN_GROUP.into()),
            ("type", "select".into()),
            ("proxies", list(select)),
        ]),
        map([
            ("name", AUTO_GROUP.into()),
            ("type", "url-test".into()),
            ("proxies", list(names.to_vec())),
            ("url", "https://www.gstatic.com/generate_204".into()),
            ("interval", 300.into()),
        ]),
    ];
    // Minimal "China direct, rest proxied" routing, like most providers ship.
    let rules = [
        "DOMAIN-SUFFIX,cn,DIRECT",
        "GEOIP,CN,DIRECT",
        &format!("MATCH,{MAIN_GROUP}"),
    ]
    .iter()
    .map(|r| Value::from(*r))
    .collect();

    map([
        ("mode", "rule".into()),
        ("proxies", Value::Sequence(proxies)),
        ("proxy-groups", Value::Sequence(groups)),
        ("rules", Value::Sequence(rules)),
    ])
}

fn parse_link(link: &str) -> Option<Mapping> {
    let (scheme, _) = link.split_once("://")?;
    match scheme.to_ascii_lowercase().as_str() {
        "ss" => parse_ss(link),
        "trojan" => parse_trojan(link),
        "vmess" => parse_vmess(link),
        "vless" => parse_vless(link),
        "hysteria2" | "hy2" => parse_hysteria2(link),
        _ => None,
    }
}

fn parse_ss(link: &str) -> Option<Mapping> {
    let rest = &link[5..];
    let (rest, fragment) = split_fragment(rest);
    let rest = rest.split(['?', '/']).next().unwrap_or(rest);
    // SIP002: userinfo@host:port, userinfo base64 or percent-encoded.
    // Legacy: base64(method:password@host:port).
    let (userinfo, server) = match rest.rsplit_once('@') {
        Some((userinfo, server)) => {
            let userinfo = decode_base64(userinfo)
                .filter(|u| u.contains(':'))
                .unwrap_or_else(|| percent_decode(userinfo));
            (userinfo, server.to_owned())
        }
        None => {
            let decoded = decode_base64(rest)?;
            let (userinfo, server) = decoded.rsplit_once('@')?;
            (userinfo.to_owned(), server.to_owned())
        }
    };
    let (cipher, password) = userinfo.split_once(':')?;
    let (host, port) = host_port(&server)?;
    Some(mapping([
        ("name", fragment.unwrap_or_else(|| host.clone()).into()),
        ("type", "ss".into()),
        ("server", host.into()),
        ("port", port.into()),
        ("cipher", cipher.into()),
        ("password", password.into()),
        ("udp", true.into()),
    ]))
}

fn parse_trojan(link: &str) -> Option<Mapping> {
    let url = reqwest::Url::parse(link).ok()?;
    let query = Query(&url);
    let mut proxy = mapping([
        ("name", fragment_or_host(&url).into()),
        ("type", "trojan".into()),
        ("server", url.host_str()?.into()),
        ("port", url.port().unwrap_or(443).into()),
        ("password", percent_decode(url.username()).into()),
        ("udp", true.into()),
    ]);
    if let Some(sni) = query.get("sni").or_else(|| query.get("peer")) {
        proxy.insert("sni".into(), sni.into());
    }
    if query.truthy("allowInsecure") || query.truthy("insecure") {
        proxy.insert("skip-cert-verify".into(), true.into());
    }
    add_transport(&mut proxy, &query);
    Some(proxy)
}

fn parse_vless(link: &str) -> Option<Mapping> {
    let url = reqwest::Url::parse(link).ok()?;
    let query = Query(&url);
    let mut proxy = mapping([
        ("name", fragment_or_host(&url).into()),
        ("type", "vless".into()),
        ("server", url.host_str()?.into()),
        ("port", url.port().unwrap_or(443).into()),
        ("uuid", url.username().into()),
        ("udp", true.into()),
    ]);
    let security = query.get("security").unwrap_or_default();
    if matches!(security.as_str(), "tls" | "reality") {
        proxy.insert("tls".into(), true.into());
    }
    if let Some(sni) = query.get("sni") {
        proxy.insert("servername".into(), sni.into());
    }
    if let Some(flow) = query.get("flow").filter(|f| !f.is_empty()) {
        proxy.insert("flow".into(), flow.into());
    }
    if let Some(fp) = query.get("fp") {
        proxy.insert("client-fingerprint".into(), fp.into());
    }
    if security == "reality" {
        let mut reality = Mapping::new();
        if let Some(key) = query.get("pbk") {
            reality.insert("public-key".into(), key.into());
        }
        if let Some(sid) = query.get("sid") {
            reality.insert("short-id".into(), sid.into());
        }
        proxy.insert("reality-opts".into(), Value::Mapping(reality));
    }
    if query.truthy("allowInsecure") {
        proxy.insert("skip-cert-verify".into(), true.into());
    }
    add_transport(&mut proxy, &query);
    Some(proxy)
}

fn parse_vmess(link: &str) -> Option<Mapping> {
    let json: serde_json::Value = serde_json::from_str(&decode_base64(&link[8..])?).ok()?;
    let field = |key: &str| -> Option<String> {
        match json.get(key)? {
            serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    };
    let server = field("add")?;
    let port: u16 = field("port")?.parse().ok()?;
    let mut proxy = mapping([
        ("name", field("ps").unwrap_or_else(|| server.clone()).into()),
        ("type", "vmess".into()),
        ("server", server.into()),
        ("port", port.into()),
        ("uuid", field("id")?.into()),
        (
            "alterId",
            field("aid")
                .and_then(|a| a.parse::<u32>().ok())
                .unwrap_or(0)
                .into(),
        ),
        (
            "cipher",
            field("scy").unwrap_or_else(|| "auto".into()).into(),
        ),
        ("udp", true.into()),
    ]);
    if field("tls").as_deref() == Some("tls") {
        proxy.insert("tls".into(), true.into());
        if let Some(sni) = field("sni").or_else(|| field("host")) {
            proxy.insert("servername".into(), sni.into());
        }
    }
    let net = field("net").unwrap_or_else(|| "tcp".into());
    let (path, host) = (field("path"), field("host"));
    insert_transport(&mut proxy, &net, path, host, None);
    Some(proxy)
}

fn parse_hysteria2(link: &str) -> Option<Mapping> {
    let url = reqwest::Url::parse(link).ok()?;
    let query = Query(&url);
    let mut proxy = mapping([
        ("name", fragment_or_host(&url).into()),
        ("type", "hysteria2".into()),
        ("server", url.host_str()?.into()),
        ("port", url.port().unwrap_or(443).into()),
        ("password", percent_decode(url.username()).into()),
    ]);
    if let Some(sni) = query.get("sni") {
        proxy.insert("sni".into(), sni.into());
    }
    if query.truthy("insecure") {
        proxy.insert("skip-cert-verify".into(), true.into());
    }
    if let Some(obfs) = query.get("obfs") {
        proxy.insert("obfs".into(), obfs.into());
        if let Some(password) = query.get("obfs-password") {
            proxy.insert("obfs-password".into(), password.into());
        }
    }
    Some(proxy)
}

fn add_transport(proxy: &mut Mapping, query: &Query<'_>) {
    let net = query.get("type").unwrap_or_else(|| "tcp".into());
    insert_transport(
        proxy,
        &net,
        query.get("path"),
        query.get("host"),
        query.get("serviceName"),
    );
}

fn insert_transport(
    proxy: &mut Mapping,
    net: &str,
    path: Option<String>,
    host: Option<String>,
    service: Option<String>,
) {
    match net {
        "ws" => {
            let mut opts = Mapping::new();
            opts.insert("path".into(), path.unwrap_or_else(|| "/".into()).into());
            if let Some(host) = host {
                opts.insert("headers".into(), mapping([("Host", host.into())]).into());
            }
            proxy.insert("network".into(), "ws".into());
            proxy.insert("ws-opts".into(), Value::Mapping(opts));
        }
        "grpc" => {
            proxy.insert("network".into(), "grpc".into());
            proxy.insert(
                "grpc-opts".into(),
                mapping([(
                    "grpc-service-name",
                    service.or(path).unwrap_or_default().into(),
                )])
                .into(),
            );
        }
        "h2" | "http" => {
            let mut opts = Mapping::new();
            opts.insert("path".into(), path.unwrap_or_else(|| "/".into()).into());
            if let Some(host) = host {
                opts.insert("host".into(), Value::Sequence(vec![host.into()]));
            }
            proxy.insert("network".into(), "h2".into());
            proxy.insert("h2-opts".into(), Value::Mapping(opts));
        }
        _ => {}
    }
}

struct Query<'a>(&'a reqwest::Url);

impl Query<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.0
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .filter(|v| !v.is_empty())
    }

    fn truthy(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }
}

fn fragment_or_host(url: &reqwest::Url) -> String {
    url.fragment()
        .map(percent_decode)
        .filter(|f| !f.trim().is_empty())
        .unwrap_or_else(|| url.host_str().unwrap_or("node").to_owned())
}

fn split_fragment(text: &str) -> (&str, Option<String>) {
    match text.split_once('#') {
        Some((rest, fragment)) => (
            rest,
            Some(percent_decode(fragment)).filter(|f| !f.is_empty()),
        ),
        None => (text, None),
    }
}

fn host_port(server: &str) -> Option<(String, u16)> {
    let (host, port) = server.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some((host.to_owned(), port.parse().ok()?))
}

fn proxy_name(proxy: &Mapping) -> String {
    proxy
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("node")
        .trim()
        .to_owned()
}

fn unique_name(taken: &[String], name: String) -> String {
    if !taken.contains(&name) {
        return name;
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|n| !taken.contains(n))
        .unwrap()
}

fn decode_base64(text: &str) -> Option<String> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let compact: String = text.split_whitespace().collect();
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .iter()
        .find_map(|engine| engine.decode(&compact).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn percent_decode(text: &str) -> String {
    reqwest::Url::parse(&format!("x://h/?v={}", text.replace('+', "%2B")))
        .ok()
        .and_then(|url| url.query_pairs().next().map(|(_, v)| v.into_owned()))
        .unwrap_or_else(|| text.to_owned())
}

fn mapping<const N: usize>(entries: [(&str, Value); N]) -> Mapping {
    let mut map = Mapping::new();
    for (key, value) in entries {
        map.insert(Value::String(key.to_owned()), value);
    }
    map
}

fn map<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Mapping(mapping(entries))
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use serde_yaml::Value;

    use super::{MAIN_GROUP, config_from_links, looks_like_links};

    fn proxies(config: &Value) -> &Vec<Value> {
        config["proxies"].as_sequence().unwrap()
    }

    #[test]
    fn parses_common_share_links() {
        let ss_userinfo =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("aes-256-gcm:pa ss");
        let vmess = base64::engine::general_purpose::STANDARD.encode(
            r#"{"v":"2","ps":"VM 香港","add":"v.example.com","port":"443","id":"uuid-1","aid":0,"net":"ws","path":"/ray","host":"cdn.example.com","tls":"tls"}"#,
        );
        let text = format!(
            "ss://{ss_userinfo}@1.2.3.4:8388#%E9%A6%99%E6%B8%AF\n\
             trojan://secret@t.example.com:443?sni=t.example.com&type=ws&path=%2Fws#Trojan\n\
             vmess://{vmess}\n\
             vless://uuid-2@r.example.com:443?security=reality&sni=www.apple.com&pbk=KEY&sid=ab&fp=chrome&flow=xtls-rprx-vision#Reality\n\
             hy2://pw@h.example.com:8443?sni=h.example.com&obfs=salamander&obfs-password=x#HY2\n\
             not-a-link\n"
        );
        assert!(looks_like_links(&text));
        let config = config_from_links(&text).unwrap();
        let list = proxies(&config);
        assert_eq!(list.len(), 5);

        assert_eq!(list[0]["name"], Value::from("香港"));
        assert_eq!(list[0]["cipher"], Value::from("aes-256-gcm"));
        assert_eq!(list[0]["password"], Value::from("pa ss"));
        assert_eq!(list[1]["ws-opts"]["path"], Value::from("/ws"));
        assert_eq!(list[2]["type"], Value::from("vmess"));
        assert_eq!(
            list[2]["ws-opts"]["headers"]["Host"],
            Value::from("cdn.example.com")
        );
        assert_eq!(list[3]["reality-opts"]["public-key"], Value::from("KEY"));
        assert_eq!(list[3]["flow"], Value::from("xtls-rprx-vision"));
        assert_eq!(list[4]["type"], Value::from("hysteria2"));

        let groups = config["proxy-groups"].as_sequence().unwrap();
        assert_eq!(groups[0]["name"], Value::from(MAIN_GROUP));
        assert_eq!(
            config["rules"].as_sequence().unwrap().last().unwrap(),
            &Value::from("MATCH,节点选择")
        );
    }

    #[test]
    fn decodes_base64_subscriptions_and_dedupes_names() {
        let body = "trojan://a@x.example.com:443#Same\ntrojan://b@y.example.com:443#Same\n";
        let encoded = base64::engine::general_purpose::STANDARD.encode(body);
        let config = config_from_links(&encoded).unwrap();
        let names: Vec<&str> = proxies(&config)
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Same", "Same 2"]);
    }

    #[test]
    fn rejects_text_without_links() {
        assert!(config_from_links("hello world").is_none());
        assert!(!looks_like_links("https://example.com/sub"));
    }
}
