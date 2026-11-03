//! Builds the config the core actually runs, from every layer:
//!
//! 1. app defaults (only what TUN needs to work, e.g. a DNS server)
//! 2. the subscription
//! 3. the user's custom config (`override.yaml`), then per-profile override
//! 4. the user's simple rules (L2), prepended so they win
//! 5. shell settings the app must control (ports, mode, TUN switch)

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::{
    config_layers::RuntimeConfigBuilder,
    i18n::t,
    network::{LOCAL_CIDRS, LocalNetwork},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    /// The domain and all its subdomains.
    DomainSuffix,
    /// Domains containing the text.
    DomainKeyword,
    /// An IP range like `10.0.0.0/8`.
    IpCidr,
    /// A program, e.g. `Telegram.exe`.
    ProcessName,
}

impl RuleKind {
    fn clash(self) -> &'static str {
        match self {
            Self::DomainSuffix => "DOMAIN-SUFFIX",
            Self::DomainKeyword => "DOMAIN-KEYWORD",
            Self::IpCidr => "IP-CIDR",
            Self::ProcessName => "PROCESS-NAME",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleAction {
    Proxy,
    Direct,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomRule {
    pub kind: RuleKind,
    pub value: String,
    pub action: RuleAction,
}

impl CustomRule {
    /// Normalise user input; reject anything that can't be one rule field.
    pub fn validated(kind: RuleKind, value: &str, action: RuleAction) -> anyhow::Result<Self> {
        let mut value = value.trim().to_owned();
        if matches!(kind, RuleKind::DomainSuffix | RuleKind::DomainKeyword) {
            // Accept pasted URLs: "https://www.example.com/path" → "www.example.com".
            if let Ok(url) = reqwest::Url::parse(&value)
                && let Some(host) = url.host_str()
            {
                value = host.to_owned();
            }
            value = value
                .trim_start_matches("*.")
                .trim_matches('.')
                .to_ascii_lowercase();
        }
        anyhow::ensure!(!value.is_empty(), "{}", t("请输入内容", "Enter a value"));
        anyhow::ensure!(
            !value.contains([',', '\n', '\r']) && !value.contains(char::is_whitespace),
            "{}",
            t("不能包含逗号或空格", "Can't contain commas or spaces")
        );
        if kind == RuleKind::IpCidr {
            let (ip, prefix) = value.split_once('/').unwrap_or((&value, ""));
            anyhow::ensure!(
                ip.parse::<std::net::IpAddr>().is_ok()
                    && (prefix.is_empty() || prefix.parse::<u8>().is_ok()),
                "{}",
                t(
                    "IP 段格式应类似 10.0.0.0/8",
                    "IP ranges should look like 10.0.0.0/8"
                )
            );
            if prefix.is_empty() {
                value = if ip.contains(':') {
                    format!("{ip}/128")
                } else {
                    format!("{ip}/32")
                };
            }
        }
        Ok(Self {
            kind,
            value,
            action,
        })
    }

    fn to_clash(&self, proxy_target: &str) -> String {
        let target = match self.action {
            RuleAction::Proxy => proxy_target,
            RuleAction::Direct => "DIRECT",
            RuleAction::Reject => "REJECT",
        };
        let mut rule = format!("{},{},{target}", self.kind.clash(), self.value);
        if self.kind == RuleKind::IpCidr {
            rule.push_str(",no-resolve");
        }
        rule
    }
}

pub struct ConfigInputs<'a> {
    pub subscription: Value,
    /// Other subscriptions merged into this one: `(display name, config)`.
    pub extra_subscriptions: Vec<(String, Value)>,
    /// Global then per-profile override, in order.
    pub overrides: Vec<Value>,
    pub custom_rules: &'a [CustomRule],
    /// The user's "不走代理的地址": domains and IP ranges.
    pub bypass: &'a [String],
    pub local: &'a LocalNetwork,
    pub mixed_port: u16,
    pub mode: &'a str,
    pub tun: bool,
    /// Capture IPv6 as well under TUN (otherwise it bypasses the tunnel).
    pub tun_ipv6: bool,
    /// Windows named pipe / unix socket path for the controller.
    pub controller_ipc: Option<&'a str>,
    /// TCP controller address (used where no IPC transport is available).
    pub controller_http: Option<(&'a str, &'a str)>,
}

pub fn generate(inputs: ConfigInputs<'_>) -> Value {
    let mut subscription = inputs.subscription;
    merge_subscriptions(&mut subscription, inputs.extra_subscriptions);
    let proxy_target = main_select_group(&subscription).unwrap_or_else(|| "GLOBAL".to_owned());

    let defaults = if inputs.tun {
        tun_defaults()
    } else {
        Value::Mapping(Mapping::new())
    };
    let mut builder = RuntimeConfigBuilder::new(defaults).push_override(subscription);
    for layer in inputs.overrides {
        if layer.is_mapping() {
            builder = builder.push_override(layer);
        }
    }
    // Local/bypass rules go above the subscription's; the user's site
    // rules (pushed after, so prepended before these) still win.
    let bypass = Bypass::new(inputs.bypass, inputs.local);
    builder = builder.push_override(mapping([("prepend-rules", bypass.rules())]));
    if !inputs.custom_rules.is_empty() {
        let rules: Vec<Value> = inputs
            .custom_rules
            .iter()
            .map(|rule| Value::String(rule.to_clash(&proxy_target)))
            .collect();
        builder = builder.push_override(mapping([("prepend-rules", Value::Sequence(rules))]));
    }

    let mut shell = mapping([
        ("mixed-port", Value::from(u64::from(inputs.mixed_port))),
        ("mode", Value::from(inputs.mode)),
    ]);
    let shell_map = shell.as_mapping_mut().expect("mapping");
    // Keep the user's node picks across restarts even if a subscription
    // turns this off (the app also restores them itself).
    shell_map.insert("profile".into(), mapping([("store-selected", true.into())]));
    if inputs.tun {
        let mut tun = mapping([
            ("enable", true.into()),
            ("route-all", true.into()),
            ("dns-hijack", true.into()),
        ]);
        let mut dns = mapping([("enable", true.into())]);
        if inputs.tun_ipv6 {
            // An IPv6 address on the device makes route-all add the IPv6
            // default route too; AAAA answers are needed for apps to use it.
            tun.as_mapping_mut()
                .expect("mapping")
                .insert("gateway-v6".into(), "fdfe:dcba:9876::1/126".into());
            dns.as_mapping_mut()
                .expect("mapping")
                .insert("ipv6".into(), true.into());
            shell_map.insert("ipv6".into(), true.into());
        }
        shell_map.insert("tun".into(), tun);
        // TUN hijacks DNS into the core, so its DNS server must be on.
        shell_map.insert("dns".into(), dns);
    } else {
        shell_map.insert("tun".into(), mapping([("enable", false.into())]));
    }
    if let Some(ipc) = inputs.controller_ipc {
        let key = if cfg!(windows) {
            "external-controller-pipe"
        } else {
            "external-controller-unix"
        };
        shell_map.insert(key.into(), ipc.into());
    }
    if let Some((addr, secret)) = inputs.controller_http {
        shell_map.insert("external-controller".into(), addr.into());
        shell_map.insert("secret".into(), secret.into());
    }

    let mut config = builder.push_override(shell).build();
    if inputs.tun {
        bypass.apply_dns(&mut config, inputs.local);
    }
    config
}

/// Addresses that must never go through a proxy: the user's list, local
/// suffixes (built-in + the machine's DNS search suffixes) and private ranges.
pub struct Bypass {
    pub domains: Vec<String>,
    pub cidrs: Vec<String>,
}

impl Bypass {
    pub fn new(user: &[String], local: &LocalNetwork) -> Self {
        let mut domains = local.local_suffixes();
        let mut cidrs: Vec<String> = LOCAL_CIDRS.iter().map(|c| (*c).to_owned()).collect();
        for entry in user {
            match parse_bypass_entry(entry) {
                Some(BypassEntry::Cidr(cidr)) => cidrs.push(cidr),
                Some(BypassEntry::Domain(domain)) => domains.push(domain),
                None => {}
            }
        }
        domains.sort();
        domains.dedup();
        cidrs.dedup();
        Self { domains, cidrs }
    }

    fn rules(&self) -> Value {
        let domains = self
            .domains
            .iter()
            .map(|d| format!("DOMAIN-SUFFIX,{d},DIRECT"));
        let cidrs = self
            .cidrs
            .iter()
            .map(|c| format!("IP-CIDR,{c},DIRECT,no-resolve"));
        Value::Sequence(domains.chain(cidrs).map(Value::from).collect())
    }

    /// TUN takes over DNS: local names must still be answered by the
    /// system's own DNS, and must get real (not fake) addresses.
    fn apply_dns(&self, config: &mut Value, local: &LocalNetwork) {
        let Some(dns) = config.get_mut("dns").and_then(Value::as_mapping_mut) else {
            return;
        };
        let patterns: Vec<String> = self.domains.iter().map(|d| format!("+.{d}")).collect();

        let filter = dns
            .entry("fake-ip-filter".into())
            .or_insert_with(|| Value::Sequence(Vec::new()));
        if let Value::Sequence(list) = filter {
            for pattern in &patterns {
                if !list.iter().any(|v| v.as_str() == Some(pattern)) {
                    list.push(pattern.clone().into());
                }
            }
        }

        if let Some(server) = local.dns_servers.first() {
            let policy = dns
                .entry("nameserver-policy".into())
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Value::Mapping(policy) = policy {
                for pattern in patterns {
                    // A subscription's own policy for the same domain wins.
                    policy
                        .entry(pattern.into())
                        .or_insert_with(|| server.clone().into());
                }
            }
        }
    }

    /// WinINet-style bypass entries (`*.lan`, `10.*`) for system-proxy mode.
    pub fn wininet_entries(&self) -> Vec<String> {
        let mut entries: Vec<String> = self
            .domains
            .iter()
            .flat_map(|d| [d.clone(), format!("*.{d}")])
            .collect();
        for cidr in &self.cidrs {
            // WinINet only understands octet wildcards; the core still
            // routes other ranges DIRECT by rule.
            let Some((ip, prefix)) = cidr.split_once('/') else {
                continue;
            };
            let octets: Vec<&str> = ip.split('.').collect();
            let keep = match prefix.parse::<u8>() {
                Ok(32) => 4,
                Ok(24) => 3,
                Ok(16) => 2,
                Ok(8) => 1,
                _ => continue,
            };
            if octets.len() == 4 {
                let mut parts: Vec<&str> = octets[..keep].to_vec();
                if keep < 4 {
                    parts.push("*");
                }
                entries.push(parts.join("."));
            }
        }
        entries
    }
}

pub enum BypassEntry {
    Domain(String),
    Cidr(String),
}

/// Normalise one "不走代理" entry: a domain (`*.example.com`, a pasted URL)
/// or an IP / CIDR. `None` if it's neither.
pub fn parse_bypass_entry(entry: &str) -> Option<BypassEntry> {
    let mut text = entry.trim().to_owned();
    if let Ok(url) = reqwest::Url::parse(&text)
        && let Some(host) = url.host_str()
    {
        text = host.to_owned();
    }
    let (ip, prefix) = text.split_once('/').unwrap_or((&text, ""));
    if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = if prefix.is_empty() {
            max
        } else {
            prefix.parse::<u8>().ok().filter(|p| *p <= max)?
        };
        return Some(BypassEntry::Cidr(format!("{addr}/{prefix}")));
    }
    let domain = text
        .trim_start_matches("*.")
        .trim_start_matches("+.")
        .trim_matches('.')
        .to_ascii_lowercase();
    let valid =
        !domain.is_empty() && domain.contains('.') || matches!(domain.as_str(), "localhost");
    (valid
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')))
    .then_some(BypassEntry::Domain(domain))
}

/// Merge other subscriptions' nodes into `base`, keeping `base`'s rules and
/// DNS. Each extra subscription becomes a selectable group `[name]` in the
/// main selector, and its nodes join `base`'s automatic groups, so "auto"
/// picks the best node across every provider.
fn merge_subscriptions(base: &mut Value, extras: Vec<(String, Value)>) {
    let main = main_select_group(base);
    for (name, extra) in extras {
        let prefix = format!("[{name}] ");
        let group_name = format!("[{name}]");

        let mut members: Vec<Value> = Vec::new();
        if let Some(Value::Sequence(proxies)) = extra.get("proxies") {
            for proxy in proxies {
                let mut proxy = proxy.clone();
                let Some(map) = proxy.as_mapping_mut() else {
                    continue;
                };
                let renamed = format!(
                    "{prefix}{}",
                    map.get("name").and_then(Value::as_str).unwrap_or("node")
                );
                map.insert("name".into(), renamed.clone().into());
                members.push(renamed.into());
                push_to_list(base, "proxies", proxy);
            }
        }
        let mut uses: Vec<Value> = Vec::new();
        if let Some(Value::Mapping(providers)) = extra.get("proxy-providers") {
            let target = base_mapping(base, "proxy-providers");
            for (key, provider) in providers {
                let renamed = format!("{prefix}{}", key.as_str().unwrap_or("provider"));
                target.insert(renamed.clone().into(), provider.clone());
                uses.push(renamed.into());
            }
        }
        if members.is_empty() && uses.is_empty() {
            continue;
        }

        if let Some(Value::Sequence(groups)) = base.get_mut("proxy-groups") {
            for group in groups.iter_mut() {
                let kind = group
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let Some(map) = group.as_mapping_mut() else {
                    continue;
                };
                let is_main = main.as_deref() == map.get("name").and_then(Value::as_str);
                if is_main {
                    append(map, "proxies", vec![group_name.clone().into()]);
                } else if matches!(
                    kind.as_str(),
                    "url-test" | "fallback" | "load-balance" | "smart"
                ) {
                    append(map, "proxies", members.clone());
                    if !uses.is_empty() {
                        append(map, "use", uses.clone());
                    }
                }
            }
        }

        let mut group = Mapping::new();
        group.insert("name".into(), group_name.into());
        group.insert("type".into(), "select".into());
        if !members.is_empty() {
            group.insert("proxies".into(), Value::Sequence(members));
        }
        if !uses.is_empty() {
            group.insert("use".into(), Value::Sequence(uses));
        }
        push_to_list(base, "proxy-groups", Value::Mapping(group));
    }
}

fn base_mapping<'a>(base: &'a mut Value, key: &str) -> &'a mut Mapping {
    let map = base.as_mapping_mut().expect("config is a mapping");
    let entry = map
        .entry(key.into())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if !entry.is_mapping() {
        *entry = Value::Mapping(Mapping::new());
    }
    entry.as_mapping_mut().expect("just made a mapping")
}

fn push_to_list(base: &mut Value, key: &str, item: Value) {
    let Some(map) = base.as_mapping_mut() else {
        return;
    };
    let entry = map
        .entry(key.into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    if let Value::Sequence(list) = entry {
        list.push(item);
    }
}

fn append(map: &mut Mapping, key: &str, items: Vec<Value>) {
    let entry = map
        .entry(key.into())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    if let Value::Sequence(list) = entry {
        list.extend(items);
    }
}

/// The first `select` group: what the user picks nodes from, and where
/// "proxy" rules should send traffic.
pub fn main_select_group(config: &Value) -> Option<String> {
    config
        .get("proxy-groups")?
        .as_sequence()?
        .iter()
        .find(|group| group.get("type").and_then(Value::as_str) == Some("select"))?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

/// Only fills gaps: a subscription's own `dns` section wins over these.
fn tun_defaults() -> Value {
    let servers = |list: &[&str]| Value::Sequence(list.iter().map(|s| Value::from(*s)).collect());
    mapping([(
        "dns",
        mapping([
            ("enable", true.into()),
            ("ipv6", false.into()),
            ("enhanced-mode", "fake-ip".into()),
            (
                "default-nameserver",
                servers(&["223.5.5.5", "119.29.29.29"]),
            ),
            (
                "nameserver",
                servers(&[
                    "https://doh.pub/dns-query",
                    "https://dns.alidns.com/dns-query",
                ]),
            ),
        ]),
    )])
}

fn mapping<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut map = Mapping::new();
    for (key, value) in entries {
        map.insert(Value::String(key.to_owned()), value);
    }
    Value::Mapping(map)
}

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use super::{Bypass, ConfigInputs, CustomRule, RuleAction, RuleKind, generate};
    use crate::network::LocalNetwork;

    static NO_LOCAL: LocalNetwork = LocalNetwork {
        search_suffixes: Vec::new(),
        dns_servers: Vec::new(),
    };

    fn subscription() -> Value {
        serde_yaml::from_str(
            "proxy-groups:\n  - {name: Auto, type: url-test}\n  - {name: 节点选择, type: select}\nrules:\n  - MATCH,节点选择\ndns:\n  nameserver: [1.1.1.1]\n",
        )
        .unwrap()
    }

    fn inputs<'a>(rules: &'a [CustomRule], tun: bool) -> ConfigInputs<'a> {
        ConfigInputs {
            subscription: subscription(),
            extra_subscriptions: vec![],
            overrides: vec![],
            custom_rules: rules,
            bypass: &[],
            local: &NO_LOCAL,
            mixed_port: 7890,
            mode: "rule",
            tun,
            tun_ipv6: false,
            controller_ipc: Some(r"\\.\pipe\x"),
            controller_http: None,
        }
    }

    #[test]
    fn custom_rules_target_main_select_group_and_come_first() {
        let rules = [
            CustomRule::validated(
                RuleKind::DomainSuffix,
                "https://www.Example.com/a",
                RuleAction::Proxy,
            )
            .unwrap(),
            CustomRule::validated(RuleKind::IpCidr, "10.0.0.1", RuleAction::Direct).unwrap(),
        ];
        let config = generate(inputs(&rules, false));
        let list: Vec<&str> = config["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        // User site rules first, then local/bypass rules, then the subscription.
        assert_eq!(list[0], "DOMAIN-SUFFIX,www.example.com,节点选择");
        assert_eq!(list[1], "IP-CIDR,10.0.0.1/32,DIRECT,no-resolve");
        assert!(list.contains(&"DOMAIN-SUFFIX,lan,DIRECT"));
        assert!(list.contains(&"IP-CIDR,192.168.0.0/16,DIRECT,no-resolve"));
        assert_eq!(list.last(), Some(&"MATCH,节点选择"));
        assert_eq!(config["tun"]["enable"], Value::from(false));
        // No TUN: DNS is left alone.
        assert!(config["dns"].get("nameserver-policy").is_none());
    }

    #[test]
    fn tun_ipv6_adds_gateway_and_aaaa_and_selection_is_kept() {
        let off = generate(inputs(&[], true));
        assert!(off["tun"].get("gateway-v6").is_none());
        assert_eq!(off["profile"]["store-selected"], Value::from(true));

        let on = generate(ConfigInputs {
            tun_ipv6: true,
            ..inputs(&[], true)
        });
        assert_eq!(
            on["tun"]["gateway-v6"],
            Value::from("fdfe:dcba:9876::1/126")
        );
        assert_eq!(on["dns"]["ipv6"], Value::from(true));
        assert_eq!(on["ipv6"], Value::from(true));
    }

    #[test]
    fn bypass_entries_route_direct_and_use_system_dns_under_tun() {
        let local = LocalNetwork {
            search_suffixes: vec!["corp.example.com".into()],
            dns_servers: vec!["192.0.2.53".into()],
        };
        let bypass = [
            "*.nas.example.net".to_owned(),
            "https://nas.example.org/".to_owned(),
            "8.8.8.0/24".to_owned(),
            "nonsense".to_owned(),
        ];
        let config = generate(ConfigInputs {
            bypass: &bypass,
            local: &local,
            ..inputs(&[], true)
        });
        let rules: Vec<&str> = config["rules"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        for expected in [
            "DOMAIN-SUFFIX,nas.example.net,DIRECT",
            "DOMAIN-SUFFIX,nas.example.org,DIRECT",
            "DOMAIN-SUFFIX,corp.example.com,DIRECT",
            "IP-CIDR,8.8.8.0/24,DIRECT,no-resolve",
        ] {
            assert!(rules.contains(&expected), "missing {expected}");
        }
        assert!(!rules.iter().any(|r| r.contains("nonsense")));

        let dns = &config["dns"];
        assert_eq!(
            dns["nameserver-policy"]["+.nas.example.net"],
            Value::from("192.0.2.53")
        );
        assert_eq!(dns["nameserver-policy"]["+.lan"], Value::from("192.0.2.53"));
        let filter: Vec<&str> = dns["fake-ip-filter"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(filter.contains(&"+.nas.example.net"));
    }

    #[test]
    fn wininet_entries_cover_domains_and_octet_ranges() {
        let bypass = Bypass::new(&["nas.example.net".to_owned()], &NO_LOCAL);
        let entries = bypass.wininet_entries();
        assert!(entries.contains(&"*.nas.example.net".to_owned()));
        assert!(entries.contains(&"nas.example.net".to_owned()));
        assert!(entries.contains(&"10.*".to_owned()));
        assert!(entries.contains(&"192.168.*".to_owned()));
    }

    #[test]
    fn merged_subscriptions_add_prefixed_nodes_to_auto_and_main_groups() {
        let base: Value = serde_yaml::from_str(
            "proxies:\n  - {name: HK, type: ss}\nproxy-groups:\n  - {name: 选择, type: select, proxies: [自动, HK]}\n  - {name: 自动, type: url-test, proxies: [HK]}\nrules:\n  - MATCH,选择\n",
        )
        .unwrap();
        let extra: Value = serde_yaml::from_str(
            "proxies:\n  - {name: JP, type: trojan}\nproxy-providers:\n  sub: {type: http, url: 'https://x'}\n",
        )
        .unwrap();
        let config = generate(ConfigInputs {
            subscription: base,
            extra_subscriptions: vec![("B".into(), extra)],
            ..inputs(&[], false)
        });

        let names: Vec<&str> = config["proxies"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["HK", "[B] JP"]);
        assert!(config["proxy-providers"].get("[B] sub").is_some());

        let groups = config["proxy-groups"].as_sequence().unwrap();
        assert_eq!(
            groups[0]["proxies"].as_sequence().unwrap().last().unwrap(),
            &Value::from("[B]")
        );
        assert_eq!(groups[1]["proxies"][1], Value::from("[B] JP"));
        assert_eq!(groups[1]["use"][0], Value::from("[B] sub"));
        assert_eq!(groups[2]["name"], Value::from("[B]"));
    }

    #[test]
    fn tun_mode_forces_tun_and_dns_but_keeps_subscription_dns_servers() {
        let config = generate(inputs(&[], true));
        assert_eq!(config["tun"]["enable"], Value::from(true));
        assert_eq!(config["tun"]["dns-hijack"], Value::from(true));
        assert_eq!(config["dns"]["enable"], Value::from(true));
        // The subscription's nameserver list replaces the default one.
        assert_eq!(config["dns"]["nameserver"][0], Value::from("1.1.1.1"));
        assert_eq!(config["dns"]["enhanced-mode"], Value::from("fake-ip"));
    }

    #[test]
    fn rejects_values_that_would_break_a_rule() {
        assert!(
            CustomRule::validated(RuleKind::DomainSuffix, "a.com,REJECT", RuleAction::Proxy)
                .is_err()
        );
        assert!(CustomRule::validated(RuleKind::IpCidr, "not-an-ip", RuleAction::Direct).is_err());
        assert!(CustomRule::validated(RuleKind::DomainSuffix, "  ", RuleAction::Direct).is_err());
    }
}
