//! Builds the config the core actually runs, from every layer:
//!
//! 1. app defaults (only what TUN needs to work, e.g. a DNS server)
//! 2. the subscription
//! 3. the user's custom config (`override.yaml`), then per-profile override
//! 4. the user's simple rules (L2), prepended so they win
//! 5. shell settings the app must control (ports, mode, TUN switch)

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::config_layers::RuntimeConfigBuilder;

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
            value = value.trim_start_matches("*.").trim_matches('.').to_ascii_lowercase();
        }
        anyhow::ensure!(!value.is_empty(), "请输入内容");
        anyhow::ensure!(
            !value.contains([',', '\n', '\r']) && !value.contains(char::is_whitespace),
            "不能包含逗号或空格"
        );
        if kind == RuleKind::IpCidr {
            let (ip, prefix) = value.split_once('/').unwrap_or((&value, ""));
            anyhow::ensure!(
                ip.parse::<std::net::IpAddr>().is_ok()
                    && (prefix.is_empty() || prefix.parse::<u8>().is_ok()),
                "IP 段格式应类似 10.0.0.0/8"
            );
            if prefix.is_empty() {
                value = if ip.contains(':') { format!("{ip}/128") } else { format!("{ip}/32") };
            }
        }
        Ok(Self { kind, value, action })
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
    /// Global then per-profile override, in order.
    pub overrides: Vec<Value>,
    pub custom_rules: &'a [CustomRule],
    pub mixed_port: u16,
    pub mode: &'a str,
    pub tun: bool,
    /// Windows named pipe / unix socket path for the controller.
    pub controller_ipc: Option<&'a str>,
    /// TCP controller address (used where no IPC transport is available).
    pub controller_http: Option<(&'a str, &'a str)>,
}

pub fn generate(inputs: ConfigInputs<'_>) -> Value {
    let proxy_target = main_select_group(&inputs.subscription).unwrap_or_else(|| "GLOBAL".to_owned());

    let defaults = if inputs.tun { tun_defaults() } else { Value::Mapping(Mapping::new()) };
    let mut builder = RuntimeConfigBuilder::new(defaults).push_override(inputs.subscription);
    for layer in inputs.overrides {
        if layer.is_mapping() {
            builder = builder.push_override(layer);
        }
    }
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
    if inputs.tun {
        shell_map.insert(
            "tun".into(),
            mapping([
                ("enable", true.into()),
                ("route-all", true.into()),
                ("dns-hijack", true.into()),
            ]),
        );
        // TUN hijacks DNS into the core, so its DNS server must be on.
        shell_map.insert("dns".into(), mapping([("enable", true.into())]));
    } else {
        shell_map.insert("tun".into(), mapping([("enable", false.into())]));
    }
    if let Some(ipc) = inputs.controller_ipc {
        let key = if cfg!(windows) { "external-controller-pipe" } else { "external-controller-unix" };
        shell_map.insert(key.into(), ipc.into());
    }
    if let Some((addr, secret)) = inputs.controller_http {
        shell_map.insert("external-controller".into(), addr.into());
        shell_map.insert("secret".into(), secret.into());
    }

    builder.push_override(shell).build()
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
            ("default-nameserver", servers(&["223.5.5.5", "119.29.29.29"])),
            ("nameserver", servers(&["https://doh.pub/dns-query", "https://dns.alidns.com/dns-query"])),
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

    use super::{ConfigInputs, CustomRule, RuleAction, RuleKind, generate};

    fn subscription() -> Value {
        serde_yaml::from_str(
            "proxy-groups:\n  - {name: Auto, type: url-test}\n  - {name: 节点选择, type: select}\nrules:\n  - MATCH,节点选择\ndns:\n  nameserver: [1.1.1.1]\n",
        )
        .unwrap()
    }

    fn inputs<'a>(rules: &'a [CustomRule], tun: bool) -> ConfigInputs<'a> {
        ConfigInputs {
            subscription: subscription(),
            overrides: vec![],
            custom_rules: rules,
            mixed_port: 7890,
            mode: "rule",
            tun,
            controller_ipc: Some(r"\\.\pipe\x"),
            controller_http: None,
        }
    }

    #[test]
    fn custom_rules_target_main_select_group_and_come_first() {
        let rules = [
            CustomRule::validated(RuleKind::DomainSuffix, "https://www.Example.com/a", RuleAction::Proxy)
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
        assert_eq!(
            list,
            [
                "DOMAIN-SUFFIX,www.example.com,节点选择",
                "IP-CIDR,10.0.0.1/32,DIRECT,no-resolve",
                "MATCH,节点选择"
            ]
        );
        assert_eq!(config["tun"]["enable"], Value::from(false));
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
        assert!(CustomRule::validated(RuleKind::DomainSuffix, "a.com,REJECT", RuleAction::Proxy).is_err());
        assert!(CustomRule::validated(RuleKind::IpCidr, "not-an-ip", RuleAction::Direct).is_err());
        assert!(CustomRule::validated(RuleKind::DomainSuffix, "  ", RuleAction::Direct).is_err());
    }
}
