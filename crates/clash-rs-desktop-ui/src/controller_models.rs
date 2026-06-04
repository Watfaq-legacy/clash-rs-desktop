
use crate::controller_endpoint::controller_client_config;
use crate::runtime_actions::{describe_core_state, managed_runtime};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverviewSnapshot {
    pub(crate) connected: bool,
    pub(crate) message: String,
    pub(crate) version: String,
    pub(crate) mode: String,
    pub(crate) log_level: String,
    pub(crate) mixed_port: String,
    pub(crate) allow_lan: String,
    pub(crate) ipv6: String,
    pub(crate) memory: String,
    pub(crate) connections: String,
    pub(crate) upload_total: String,
    pub(crate) download_total: String,
    pub(crate) upload_speed: String,
    pub(crate) download_speed: String,
    pub(crate) lifecycle: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProxyRow {
    pub(crate) name: String,
    pub(crate) proxy_type: String,
    pub(crate) current: String,
    pub(crate) members: String,
    pub(crate) member_names: Vec<String>,
    pub(crate) delay: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProxySnapshot {
    pub(crate) message: String,
    pub(crate) rows: Vec<ProxyRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProviderRow {
    pub(crate) scope: String,
    pub(crate) name: String,
    pub(crate) provider_type: String,
    pub(crate) vehicle: String,
    pub(crate) behavior: String,
    pub(crate) format: String,
    pub(crate) count: String,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProviderSnapshot {
    pub(crate) message: String,
    pub(crate) rows: Vec<ProviderRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RuleRow {
    pub(crate) rule_type: String,
    pub(crate) payload: String,
    pub(crate) proxy: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RulesSnapshot {
    pub(crate) message: String,
    pub(crate) rows: Vec<RuleRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectionRow {
    pub(crate) id: String,
    pub(crate) host: String,
    pub(crate) asn: String,
    pub(crate) process: String,
    pub(crate) network: String,
    pub(crate) rule: String,
    pub(crate) chains: String,
    pub(crate) upload: String,
    pub(crate) download: String,
    pub(crate) duration: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectionsSnapshot {
    pub(crate) message: String,
    pub(crate) upload_total: String,
    pub(crate) download_total: String,
    pub(crate) rows: Vec<ConnectionRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DnsAnswerRow {
    pub(crate) name: String,
    pub(crate) record_type: String,
    pub(crate) ttl: String,
    pub(crate) data: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DnsSnapshot {
    pub(crate) message: String,
    pub(crate) rows: Vec<DnsAnswerRow>,
}

impl OverviewSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            connected: false,
            message: "Refresh Overview to load controller state.".to_owned(),
            version: "Offline".to_owned(),
            mode: "-".to_owned(),
            log_level: "-".to_owned(),
            mixed_port: "-".to_owned(),
            allow_lan: "-".to_owned(),
            ipv6: "-".to_owned(),
            memory: "-".to_owned(),
            connections: "-".to_owned(),
            upload_total: "-".to_owned(),
            download_total: "-".to_owned(),
            upload_speed: "-".to_owned(),
            download_speed: "-".to_owned(),
            lifecycle: lifecycle_status(),
        }
    }
}

impl ProxySnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            message: "Refresh Proxies to load proxy groups and static proxies.".to_owned(),
            rows: Vec::new(),
        }
    }
}

impl ProviderSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            message: "Refresh Providers to load proxy providers.".to_owned(),
            rows: Vec::new(),
        }
    }
}

impl RulesSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            message: "Refresh Rules to load rule table.".to_owned(),
            rows: Vec::new(),
        }
    }
}

impl ConnectionsSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            message: "Refresh Connections to load active connections.".to_owned(),
            upload_total: "-".to_owned(),
            download_total: "-".to_owned(),
            rows: Vec::new(),
        }
    }
}

impl DnsSnapshot {
    pub(crate) fn empty() -> Self {
        Self {
            message: "Enter a hostname and query DNS to see answer records.".to_owned(),
            rows: Vec::new(),
        }
    }
}

pub(crate) fn load_overview_snapshot() -> OverviewSnapshot {
    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            let client = controller_client();
            let version = client.get_version().await;
            let configs = client.get_configs().await;
            let memory = client.get_memory().await;
            let connections = client.get_connections().await;

            let connected =
                version.is_ok() || configs.is_ok() || memory.is_ok() || connections.is_ok();
            OverviewSnapshot {
                connected,
                message: if connected {
                    "Overview refreshed.".to_owned()
                } else {
                    format!(
                        "Controller is offline or unreachable: {}",
                        version
                            .as_ref()
                            .err()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "no controller response".to_owned())
                    )
                },
                version: version
                    .map(|value| format!("v{}", value.version))
                    .unwrap_or_else(|_| "Offline".to_owned()),
                mode: configs
                    .as_ref()
                    .ok()
                    .and_then(|value| value.mode.clone())
                    .unwrap_or_else(|| "-".to_owned()),
                log_level: configs
                    .as_ref()
                    .ok()
                    .and_then(|value| value.log_level.clone())
                    .unwrap_or_else(|| "-".to_owned()),
                mixed_port: configs
                    .as_ref()
                    .ok()
                    .and_then(|value| value.mixed_port.map(|port| port.to_string()))
                    .unwrap_or_else(|| "-".to_owned()),
                allow_lan: configs
                    .as_ref()
                    .ok()
                    .and_then(|value| value.allow_lan.map(on_off))
                    .unwrap_or_else(|| "-".to_owned()),
                ipv6: configs
                    .as_ref()
                    .ok()
                    .and_then(|value| value.ipv6.map(on_off))
                    .unwrap_or_else(|| "-".to_owned()),
                memory: memory
                    .ok()
                    .and_then(|value| value.inuse.map(format_bytes))
                    .unwrap_or_else(|| "-".to_owned()),
                connections: connections
                    .as_ref()
                    .ok()
                    .map(|value| value.connections.len().to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                upload_total: connections
                    .as_ref()
                    .ok()
                    .and_then(|value| value.upload_total.map(format_bytes))
                    .unwrap_or_else(|| "-".to_owned()),
                download_total: connections
                    .as_ref()
                    .ok()
                    .and_then(|value| value.download_total.map(format_bytes))
                    .unwrap_or_else(|| "-".to_owned()),
                upload_speed: "-".to_owned(),
                download_speed: "-".to_owned(),
                lifecycle: lifecycle_status(),
            }
        }),
        Err(error) => OverviewSnapshot {
            message: format!("Overview refresh failed: {error}"),
            ..OverviewSnapshot::empty()
        },
    }
}

fn lifecycle_status() -> String {
    match managed_runtime().lock() {
        Ok(mut runtime) => {
            if let Some(manager) = runtime.as_mut() {
                let state = manager.state();
                if !matches!(state, clash_rs_desktop_core::CoreState::Stopped) {
                    return describe_core_state(&state);
                }
            }
            orphan_core_status().unwrap_or_else(|| "stopped".to_owned())
        }
        Err(error) => format!("status unavailable ({error})"),
    }
}

fn orphan_core_status() -> Option<String> {
    let paths = crate::paths::reactor_paths().ok()?;
    let pid = clash_rs_desktop_core::process::read_core_pid(&paths.core_pid_path())?;
    if clash_rs_desktop_core::process::pid_is_alive(pid) {
        Some(format!("running (orphan, pid: {pid})"))
    } else {
        clash_rs_desktop_core::process::clear_core_pid(&paths.core_pid_path());
        None
    }
}

pub(crate) fn load_proxy_snapshot() -> ProxySnapshot {
    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            match controller_client().get_proxies().await {
                Ok(collection) => {
                    let mut rows = collection
                        .proxies
                        .iter()
                        .map(|(name, proxy)| ProxyRow {
                            name: name.clone(),
                            proxy_type: json_string(proxy, "type"),
                            current: json_string(proxy, "now"),
                            members: json_array_len(proxy, "all").to_string(),
                            member_names: json_string_array(proxy, "all"),
                            delay: latest_delay(proxy),
                        })
                        .collect::<Vec<_>>();
                    rows.sort_by(|left, right| left.name.cmp(&right.name));
                    ProxySnapshot {
                        message: format!("Loaded {} proxies and groups.", rows.len()),
                        rows,
                    }
                }
                Err(error) => ProxySnapshot {
                    message: format!("Refresh proxies failed: {error}"),
                    rows: Vec::new(),
                },
            }
        }),
        Err(error) => ProxySnapshot {
            message: format!("Refresh proxies failed: {error}"),
            rows: Vec::new(),
        },
    }
}

pub(crate) fn load_traffic_speed() -> (String, String) {
    use crate::controller_actions::reactor_stream_messages;
    match reactor_stream_messages("/ws/traffic", 30, 2) {
        Ok(messages) => {
            for line in messages.iter().rev() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    let up = value.get("up").and_then(serde_json::Value::as_u64);
                    let down = value.get("down").and_then(serde_json::Value::as_u64);
                    if up.is_some() || down.is_some() {
                        return (
                            up.map(|v| format!("{}/s", format_bytes(v)))
                                .unwrap_or_else(|| "-".to_owned()),
                            down.map(|v| format!("{}/s", format_bytes(v)))
                                .unwrap_or_else(|| "-".to_owned()),
                        );
                    }
                }
            }
            ("-".to_owned(), "-".to_owned())
        }
        Err(_) => ("-".to_owned(), "-".to_owned()),
    }
}

pub(crate) fn load_provider_snapshot() -> ProviderSnapshot {
    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            let client = controller_client();
            let proxy_providers = client.get_proxy_providers().await;
            let rule_providers = client.get_rule_providers().await;

            if proxy_providers.is_err() && rule_providers.is_err() {
                return ProviderSnapshot {
                    message: format!(
                        "Refresh providers failed: proxy providers ({}) rule providers ({})",
                        proxy_providers
                            .err()
                            .map(|error| error.to_string())
                            .unwrap_or_else(|| "ok".to_owned()),
                        rule_providers
                            .err()
                            .map(|error| error.to_string())
                            .unwrap_or_else(|| "ok".to_owned())
                    ),
                    rows: Vec::new(),
                };
            }

            let mut rows = Vec::new();
            if let Ok(collection) = proxy_providers {
                rows.extend(
                    collection
                        .providers
                        .iter()
                        .map(|(name, provider)| ProviderRow {
                            scope: "Proxy".to_owned(),
                            name: name.clone(),
                            provider_type: json_string(provider, "type"),
                            vehicle: json_string(provider, "vehicleType"),
                            behavior: "-".to_owned(),
                            format: "-".to_owned(),
                            count: json_array_len(provider, "proxies").to_string(),
                            updated_at: json_string(provider, "updatedAt"),
                        }),
                );
            }
            if let Ok(collection) = rule_providers {
                rows.extend(
                    collection
                        .providers
                        .iter()
                        .map(|(name, provider)| ProviderRow {
                            scope: "Rule".to_owned(),
                            name: name.clone(),
                            provider_type: json_string(provider, "type"),
                            vehicle: json_string(provider, "vehicleType"),
                            behavior: json_string(provider, "behavior"),
                            format: json_string(provider, "format"),
                            count: json_array_len(provider, "rules").to_string(),
                            updated_at: json_string(provider, "updatedAt"),
                        }),
                );
            }
            rows.sort_by(|left, right| left.name.cmp(&right.name));

            ProviderSnapshot {
                message: format!("Loaded {} providers (proxy + rule).", rows.len()),
                rows,
            }
        }),
        Err(error) => ProviderSnapshot {
            message: format!("Refresh providers failed: {error}"),
            rows: Vec::new(),
        },
    }
}

pub(crate) fn load_rules_snapshot() -> RulesSnapshot {
    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            match controller_client().get_rules().await {
                Ok(response) => RulesSnapshot {
                    message: format!("Loaded {} rules.", response.rules.len()),
                    rows: response
                        .rules
                        .iter()
                        .map(|rule| RuleRow {
                            rule_type: json_string(rule, "type"),
                            payload: json_string(rule, "payload"),
                            proxy: json_string(rule, "proxy"),
                        })
                        .collect(),
                },
                Err(error) => RulesSnapshot {
                    message: format!("Refresh rules failed: {error}"),
                    rows: Vec::new(),
                },
            }
        }),
        Err(error) => RulesSnapshot {
            message: format!("Refresh rules failed: {error}"),
            rows: Vec::new(),
        },
    }
}

pub(crate) fn load_connections_snapshot() -> ConnectionsSnapshot {
    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            match controller_client().get_connections().await {
                Ok(response) => ConnectionsSnapshot {
                    message: format!("Loaded {} active connections.", response.connections.len()),
                    upload_total: response
                        .upload_total
                        .map(format_bytes)
                        .unwrap_or_else(|| "-".to_owned()),
                    download_total: response
                        .download_total
                        .map(format_bytes)
                        .unwrap_or_else(|| "-".to_owned()),
                    rows: response
                        .connections
                        .iter()
                        .map(|connection| ConnectionRow {
                            id: json_string(connection, "id"),
                            host: nested_json_string(connection, &["metadata", "host"])
                                .or_else(|| {
                                    nested_json_string(connection, &["metadata", "destinationIP"])
                                })
                                .unwrap_or_else(|| "-".to_owned()),
                            asn: nested_json_string(connection, &["metadata", "asn"])
                                .or_else(|| {
                                    nested_json_string(connection, &["metadata", "sourceIPASN"])
                                })
                                .unwrap_or_else(|| "-".to_owned()),
                            process: nested_json_string(connection, &["metadata", "process"])
                                .or_else(|| {
                                    nested_json_string(connection, &["metadata", "processPath"])
                                })
                                .unwrap_or_else(|| "-".to_owned()),
                            network: format!(
                                "{}/{}",
                                nested_json_string(connection, &["metadata", "network"])
                                    .unwrap_or_else(|| "-".to_owned()),
                                nested_json_string(connection, &["metadata", "type"])
                                    .unwrap_or_else(|| "-".to_owned())
                            ),
                            rule: json_string(connection, "rule"),
                            chains: json_array_join(connection, "chains"),
                            upload: json_u64(connection, "upload")
                                .map(format_bytes)
                                .unwrap_or_else(|| "0 B".to_owned()),
                            download: json_u64(connection, "download")
                                .map(format_bytes)
                                .unwrap_or_else(|| "0 B".to_owned()),
                            duration: json_string(connection, "start"),
                        })
                        .collect(),
                },
                Err(error) => ConnectionsSnapshot {
                    message: format!("Refresh connections failed: {error}"),
                    upload_total: "-".to_owned(),
                    download_total: "-".to_owned(),
                    rows: Vec::new(),
                },
            }
        }),
        Err(error) => ConnectionsSnapshot {
            message: format!("Refresh connections failed: {error}"),
            upload_total: "-".to_owned(),
            download_total: "-".to_owned(),
            rows: Vec::new(),
        },
    }
}

pub(crate) fn load_dns_snapshot(name: &str, record_type: &str) -> DnsSnapshot {
    let name = name.trim();
    let record_type = record_type.trim();
    if name.is_empty() {
        return DnsSnapshot {
            message: "DNS query requires a hostname.".to_owned(),
            rows: Vec::new(),
        };
    }

    match runtime() {
        Ok(runtime) => runtime.block_on(async {
            match controller_client().query_dns(name, record_type).await {
                Ok(value) => {
                    let rows = value
                        .get("Answer")
                        .and_then(serde_json::Value::as_array)
                        .map(|answers| {
                            answers
                                .iter()
                                .map(|answer| DnsAnswerRow {
                                    name: json_string(answer, "name"),
                                    record_type: answer
                                        .get("type")
                                        .and_then(serde_json::Value::as_u64)
                                        .map(dns_type_name)
                                        .unwrap_or_else(|| "-".to_owned()),
                                    ttl: answer
                                        .get("TTL")
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|ttl| format!("{ttl}s"))
                                        .unwrap_or_else(|| "-".to_owned()),
                                    data: json_string(answer, "data"),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    DnsSnapshot {
                        message: if rows.is_empty() {
                            format!("DNS query returned no answers for {name} {record_type}.")
                        } else {
                            format!(
                                "DNS query returned {} answers for {name} {record_type}.",
                                rows.len()
                            )
                        },
                        rows,
                    }
                }
                Err(error) => DnsSnapshot {
                    message: format!("DNS query failed: {error}"),
                    rows: Vec::new(),
                },
            }
        }),
        Err(error) => DnsSnapshot {
            message: format!("DNS query failed: {error}"),
            rows: Vec::new(),
        },
    }
}

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

fn controller_client() -> clash_rs_desktop_core::ClashIpcClient {
    clash_rs_desktop_core::ClashIpcClient::new(controller_client_config())
}

fn on_off(value: bool) -> String {
    if value { "On" } else { "Off" }.to_owned()
}

fn format_bytes(value: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if value >= GIB {
        format!("{:.2} GiB", value as f64 / GIB as f64)
    } else if value >= MIB {
        format!("{:.1} MiB", value as f64 / MIB as f64)
    } else if value >= KIB {
        format!("{:.1} KiB", value as f64 / KIB as f64)
    } else {
        format!("{value} B")
    }
}

fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
        .to_owned()
}

fn nested_json_string(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str().map(ToOwned::to_owned)
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn json_array_len(value: &serde_json::Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn json_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn json_array_join(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" > ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_owned())
}

fn latest_delay(value: &serde_json::Value) -> String {
    value
        .get("history")
        .and_then(serde_json::Value::as_array)
        .and_then(|history| history.last())
        .and_then(|item| item.get("delay"))
        .and_then(serde_json::Value::as_u64)
        .map(|delay| {
            if delay == 0 {
                "Timeout".to_owned()
            } else {
                format!("{delay} ms")
            }
        })
        .unwrap_or_else(|| "-".to_owned())
}

fn dns_type_name(value: u64) -> String {
    match value {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        _ => return value.to_string(),
    }
    .to_owned()
}
