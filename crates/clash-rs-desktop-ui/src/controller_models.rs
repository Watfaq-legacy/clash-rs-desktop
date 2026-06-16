
use crate::controller_endpoint::controller_client_config;
use clash_rs_desktop_core::{ServiceCommand, ServiceResponse};

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
            lifecycle: "stopped".to_owned(),
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
            let timeout = std::time::Duration::from_secs(3);
            let traffic_window = std::time::Duration::from_secs(2);
            let (version, configs, memory, connections, traffic) = tokio::join!(
                tokio::time::timeout(timeout, client.get_version()),
                tokio::time::timeout(timeout, client.get_configs()),
                tokio::time::timeout(timeout, client.get_memory()),
                tokio::time::timeout(timeout, client.get_connections()),
                client.sample_websocket_text("/ws/traffic", 1, traffic_window),
            );
            let version = version.unwrap_or_else(|_| Err(anyhow::anyhow!("timed out")));
            let configs = configs.unwrap_or_else(|_| Err(anyhow::anyhow!("timed out")));
            let memory = memory.unwrap_or_else(|_| Err(anyhow::anyhow!("timed out")));
            let connections = connections.unwrap_or_else(|_| Err(anyhow::anyhow!("timed out")));
            let (upload_speed, download_speed) = parse_traffic_speeds(
                traffic.ok().and_then(|messages| messages.into_iter().next()).as_deref(),
            );

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
                upload_speed,
                download_speed,
                lifecycle: lifecycle_status().await,
            }
        }),
        Err(error) => OverviewSnapshot {
            message: format!("Overview refresh failed: {error}"),
            ..OverviewSnapshot::empty()
        },
    }
}

async fn lifecycle_status() -> String {
    use clash_rs_desktop_core::pipe_transport::ServicePipeClient;
    let client = ServicePipeClient::new(clash_rs_desktop_core::service::SERVICE_PIPE);
    match client.send(&ServiceCommand::Status).await {
        Ok(ServiceResponse::State { state }) => match state {
            clash_rs_desktop_core::CoreState::Running { .. } => "running".to_owned(),
            _ => "stopped".to_owned(),
        },
        _ => "stopped".to_owned(),
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
                Ok(response) => connections_snapshot_from_response(response),
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

/// Parse one `/connections` WebSocket frame (same JSON shape the REST endpoint
/// returns) into a snapshot. Returns `None` when the frame isn't a valid
/// connections payload so the caller can skip it rather than blank the table.
pub(crate) fn connections_snapshot_from_ws_frame(frame: &str) -> Option<ConnectionsSnapshot> {
    let response = serde_json::from_str::<clash_rs_desktop_core::ConnectionsResponse>(frame).ok()?;
    Some(connections_snapshot_from_response(response))
}

fn connections_snapshot_from_response(
    response: clash_rs_desktop_core::ConnectionsResponse,
) -> ConnectionsSnapshot {
    ConnectionsSnapshot {
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
            .map(connection_row_from_json)
            .collect(),
    }
}

fn connection_row_from_json(connection: &serde_json::Value) -> ConnectionRow {
    ConnectionRow {
        id: json_string(connection, "id"),
        host: nested_json_string(connection, &["metadata", "host"])
            .or_else(|| nested_json_string(connection, &["metadata", "destinationIP"]))
            .unwrap_or_else(|| "-".to_owned()),
        asn: nested_json_string(connection, &["metadata", "asn"])
            .or_else(|| nested_json_string(connection, &["metadata", "sourceIPASN"]))
            .unwrap_or_else(|| "-".to_owned()),
        process: nested_json_string(connection, &["metadata", "process"])
            .or_else(|| nested_json_string(connection, &["metadata", "processPath"]))
            .unwrap_or_else(|| "-".to_owned()),
        network: format!(
            "{}/{}",
            nested_json_string(connection, &["metadata", "network"])
                .unwrap_or_else(|| "-".to_owned()),
            nested_json_string(connection, &["metadata", "type"]).unwrap_or_else(|| "-".to_owned())
        ),
        rule: json_string(connection, "rule"),
        chains: json_array_join(connection, "chains"),
        upload: json_u64(connection, "upload")
            .map(format_bytes)
            .unwrap_or_else(|| "0 B".to_owned()),
        download: json_u64(connection, "download")
            .map(format_bytes)
            .unwrap_or_else(|| "0 B".to_owned()),
        duration: format_duration_since(&json_string(connection, "start")),
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

/// Parse a single `/ws/traffic` frame (`{"up": <bytes/s>, "down": <bytes/s>}`)
/// into display strings. Returns `("-", "-")` when no frame was sampled, e.g.
/// because the core is offline or produced no traffic tick within the window.
fn parse_traffic_speeds(frame: Option<&str>) -> (String, String) {
    let Some(frame) = frame else {
        return ("-".to_owned(), "-".to_owned());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(frame) else {
        return ("-".to_owned(), "-".to_owned());
    };
    let speed = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .map(|rate| format!("{}/s", format_bytes(rate)))
            .unwrap_or_else(|| "-".to_owned())
    };
    (speed("up"), speed("down"))
}

/// Parse one `/ws/traffic` subscription frame into `(upload, download)` display
/// strings. Returns `None` when the frame isn't valid traffic JSON so the
/// caller can leave the last-known speeds in place instead of blanking them.
pub(crate) fn traffic_speeds_from_ws_frame(frame: &str) -> Option<(String, String)> {
    if serde_json::from_str::<serde_json::Value>(frame).is_err() {
        return None;
    }
    Some(parse_traffic_speeds(Some(frame)))
}

/// Parse one `/ws/traffic` frame into raw `(up_bytes_per_sec, down_bytes_per_sec)`
/// for charting. Returns `None` on a malformed frame.
pub(crate) fn traffic_bytes_from_ws_frame(frame: &str) -> Option<(u64, u64)> {
    let value = serde_json::from_str::<serde_json::Value>(frame).ok()?;
    let up = value.get("up").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let down = value
        .get("down")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    Some((up, down))
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

/// Render the elapsed time since an RFC 3339 `start` timestamp (the form the
/// Clash controller emits for a connection's open time). Falls back to the raw
/// string when it can't be parsed so no information is lost.
fn format_duration_since(start: &str) -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    let trimmed = start.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return "-".to_owned();
    }
    let Ok(started) = OffsetDateTime::parse(trimmed, &Rfc3339) else {
        return trimmed.to_owned();
    };
    let elapsed = OffsetDateTime::now_utc() - started;
    let secs = elapsed.whole_seconds();
    if secs < 0 {
        return "0s".to_owned();
    }
    let (days, hours, minutes, seconds) = (
        secs / 86_400,
        (secs % 86_400) / 3_600,
        (secs % 3_600) / 60,
        secs % 60,
    );
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
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

#[cfg(test)]
mod tests {
    use super::{
        connection_row_from_json, connections_snapshot_from_ws_frame, dns_type_name, format_bytes,
        format_duration_since, json_array_join, json_string, latest_delay, nested_json_string,
        parse_traffic_speeds, traffic_speeds_from_ws_frame,
    };
    use serde_json::json;

    #[test]
    fn traffic_speeds_parse_up_and_down() {
        let (up, down) = parse_traffic_speeds(Some(r#"{"up": 2048, "down": 1048576}"#));
        assert_eq!(up, "2.0 KiB/s");
        assert_eq!(down, "1.0 MiB/s");
    }

    #[test]
    fn traffic_speeds_fall_back_when_absent_or_invalid() {
        assert_eq!(parse_traffic_speeds(None), ("-".to_owned(), "-".to_owned()));
        assert_eq!(
            parse_traffic_speeds(Some("not json")),
            ("-".to_owned(), "-".to_owned())
        );
    }

    #[test]
    fn traffic_ws_frame_rejects_non_json_but_accepts_valid() {
        assert!(traffic_speeds_from_ws_frame("garbage").is_none());
        let parsed = traffic_speeds_from_ws_frame(r#"{"up":0,"down":512}"#);
        assert_eq!(parsed, Some(("0 B/s".to_owned(), "512 B/s".to_owned())));
    }

    #[test]
    fn duration_blank_or_unparseable_is_preserved() {
        assert_eq!(format_duration_since(""), "-");
        assert_eq!(format_duration_since("-"), "-");
        // Not RFC 3339 — fall back to the raw string rather than lying.
        assert_eq!(format_duration_since("yesterday"), "yesterday");
    }

    #[test]
    fn duration_in_the_future_clamps_to_zero() {
        assert_eq!(format_duration_since("2999-01-01T00:00:00Z"), "0s");
    }

    #[test]
    fn format_bytes_picks_unit_by_magnitude() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
    }

    #[test]
    fn dns_type_names_map_known_and_passthrough_unknown() {
        assert_eq!(dns_type_name(1), "A");
        assert_eq!(dns_type_name(28), "AAAA");
        assert_eq!(dns_type_name(5), "CNAME");
        // Unknown numeric types pass through as their number.
        assert_eq!(dns_type_name(999), "999");
    }

    #[test]
    fn json_string_returns_dash_for_missing_or_empty() {
        let value = json!({ "present": "x", "blank": "" });
        assert_eq!(json_string(&value, "present"), "x");
        assert_eq!(json_string(&value, "blank"), "-");
        assert_eq!(json_string(&value, "missing"), "-");
    }

    #[test]
    fn nested_json_string_walks_path() {
        let value = json!({ "metadata": { "host": "example.com" } });
        assert_eq!(
            nested_json_string(&value, &["metadata", "host"]).as_deref(),
            Some("example.com")
        );
        assert_eq!(nested_json_string(&value, &["metadata", "absent"]), None);
        assert_eq!(nested_json_string(&value, &["nope", "host"]), None);
    }

    #[test]
    fn json_array_join_uses_chain_separator() {
        let value = json!({ "chains": ["proxy-a", "proxy-b"] });
        assert_eq!(json_array_join(&value, "chains"), "proxy-a > proxy-b");
        // Empty / missing arrays collapse to a dash.
        assert_eq!(json_array_join(&json!({ "chains": [] }), "chains"), "-");
        assert_eq!(json_array_join(&json!({}), "chains"), "-");
    }

    #[test]
    fn latest_delay_reads_last_history_entry() {
        let value = json!({ "history": [ { "delay": 100 }, { "delay": 240 } ] });
        assert_eq!(latest_delay(&value), "240 ms");
        // A zero delay is a timeout, not "0 ms".
        assert_eq!(latest_delay(&json!({ "history": [ { "delay": 0 } ] })), "Timeout");
        // No history at all.
        assert_eq!(latest_delay(&json!({})), "-");
    }

    #[test]
    fn connection_row_falls_back_through_metadata_keys() {
        // host missing but destinationIP present; process missing but processPath present.
        let value = json!({
            "id": "conn-1",
            "rule": "DOMAIN-SUFFIX",
            "upload": 2048,
            "download": 0,
            "metadata": {
                "destinationIP": "192.0.2.53",
                "processPath": "C:/app.exe",
                "network": "tcp",
                "type": "HTTPS"
            }
        });
        let row = connection_row_from_json(&value);
        assert_eq!(row.id, "conn-1");
        assert_eq!(row.host, "192.0.2.53");
        assert_eq!(row.process, "C:/app.exe");
        assert_eq!(row.network, "tcp/HTTPS");
        assert_eq!(row.upload, "2.0 KiB");
        assert_eq!(row.download, "0 B");
        // No asn metadata at all → dash.
        assert_eq!(row.asn, "-");
    }

    #[test]
    fn connections_ws_frame_parses_full_snapshot() {
        let frame = r#"{
            "uploadTotal": 4096,
            "downloadTotal": 1024,
            "connections": [
                { "id": "a", "metadata": { "host": "h1" } },
                { "id": "b", "metadata": { "host": "h2" } }
            ]
        }"#;
        let snapshot = connections_snapshot_from_ws_frame(frame).expect("valid frame");
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(snapshot.rows[0].id, "a");
        assert_eq!(snapshot.rows[0].host, "h1");
        assert_eq!(snapshot.upload_total, "4.0 KiB");
        assert_eq!(snapshot.download_total, "1.0 KiB");
    }

    #[test]
    fn connections_ws_frame_rejects_garbage() {
        assert!(connections_snapshot_from_ws_frame("not json").is_none());
    }
}


