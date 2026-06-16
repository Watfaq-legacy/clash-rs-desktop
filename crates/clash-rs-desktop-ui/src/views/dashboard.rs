use windows_reactor::*;

use std::sync::{Mutex, OnceLock};

use crate::controller_actions::{
    reactor_close_all_connections_text, reactor_close_connection_text,
    reactor_group_delay_text, reactor_healthcheck_provider_proxy_text,
    reactor_healthcheck_proxy_provider_text, reactor_patch_configs_text,
    reactor_reload_configs_text,
    reactor_select_proxy_text, reactor_stream_messages, reactor_subscribe_messages,
    reactor_update_proxy_provider_text, reactor_update_rule_provider_text,
};
use crate::controller_models::{
    ConnectionRow, ConnectionsSnapshot, DnsSnapshot, OverviewSnapshot, ProviderSnapshot,
    ProxySnapshot, ProxyRow, RulesSnapshot, connections_snapshot_from_ws_frame,
    load_connections_snapshot, load_dns_snapshot, load_overview_snapshot, load_provider_snapshot,
    load_proxy_snapshot, load_rules_snapshot, traffic_speeds_from_ws_frame,
};
use crate::runtime_actions::{
    reactor_restart_core_text, reactor_start_core_text, reactor_stop_core_text,
};
use crate::views::{
    design::{
        HAIRLINE, Tone, ToastQueue, empty_state as design_empty_state, hero_card, metric_card,
        metric_grid, radius, section, selectable_text, spacing, status_pill, toolbar, type_scale,
    },
    output::{SafetyState, confirm_label, guarded_action},
};

pub(crate) fn looks_like_failure(text: &str) -> bool {
    // Only inspect the first line. Action results are shaped as
    // "<label>: ok\n<pretty-printed JSON>" on success or "<label> failed: ..."
    // on failure, so scanning the whole string would match words like "error"
    // appearing inside a successful response body (a proxy/host/rule name).
    let lower = text.lines().next().unwrap_or(text).to_ascii_lowercase();
    lower.contains("failed")
        || lower.contains("error")
        || lower.starts_with("could not")
        || lower.contains("denied")
}

pub(crate) fn report_action(toasts: &ToastQueue, success: &str, result: &str) {
    if looks_like_failure(result) {
        let title = format!("{success} failed");
        toasts.push(title, trim_one_line(result), Tone::Critical);
    }
}

/// Run a blocking action on a worker thread. Marks busy on UI thread,
/// runs `op`, then refreshes the overview snapshot and reports failures.
pub(crate) fn spawn_action<F>(
    set_action_busy: AsyncSetState<bool>,
    set_overview: AsyncSetState<OverviewSnapshot>,
    toasts: ToastQueue,
    label: &'static str,
    op: F,
) where
    F: FnOnce() -> String + Send + 'static,
{
    set_action_busy.call(true);
    std::thread::spawn(move || {
        let result = op();
        let snapshot = load_overview_snapshot();
        set_overview.call(snapshot);
        set_action_busy.call(false);
        report_action(&toasts, label, &result);
    });
}

pub(crate) fn trim_one_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.len() > 140 {
        format!("{}…", &line[..140])
    } else {
        line.to_owned()
    }
}

#[derive(Clone)]
pub(crate) struct DashboardState {
    pub(crate) overview: OverviewSnapshot,
    pub(crate) set_overview: AsyncSetState<OverviewSnapshot>,
    pub(crate) proxies: ProxySnapshot,
    pub(crate) set_proxies: AsyncSetState<ProxySnapshot>,
    pub(crate) providers: ProviderSnapshot,
    pub(crate) set_providers: AsyncSetState<ProviderSnapshot>,
    pub(crate) rules: RulesSnapshot,
    pub(crate) set_rules: AsyncSetState<RulesSnapshot>,
    pub(crate) connections: ConnectionsSnapshot,
    pub(crate) set_connections: AsyncSetState<ConnectionsSnapshot>,
    pub(crate) dns: DnsSnapshot,
    pub(crate) set_dns: AsyncSetState<DnsSnapshot>,
    pub(crate) dns_name: String,
    pub(crate) set_dns_name: SetState<String>,
    pub(crate) dns_type: String,
    pub(crate) set_dns_type: SetState<String>,
    pub(crate) config_mode: String,
    pub(crate) set_config_mode: SetState<String>,
    pub(crate) config_log_level: String,
    pub(crate) set_config_log_level: SetState<String>,
    pub(crate) config_mixed_port: String,
    pub(crate) set_config_mixed_port: SetState<String>,
    pub(crate) config_bind_address: String,
    pub(crate) set_config_bind_address: SetState<String>,
    pub(crate) config_allow_lan: bool,
    pub(crate) set_config_allow_lan: SetState<bool>,
    pub(crate) config_allow_lan_dirty: bool,
    pub(crate) set_config_allow_lan_dirty: SetState<bool>,
    pub(crate) config_ipv6: bool,
    pub(crate) set_config_ipv6: SetState<bool>,
    pub(crate) config_ipv6_dirty: bool,
    pub(crate) set_config_ipv6_dirty: SetState<bool>,
    pub(crate) config_reload_path: String,
    pub(crate) set_config_reload_path: SetState<String>,
    pub(crate) proxy_provider: String,
    pub(crate) set_proxy_provider: SetState<String>,
    pub(crate) provider_proxy: String,
    pub(crate) set_provider_proxy: SetState<String>,
    pub(crate) rule_provider: String,
    pub(crate) set_rule_provider: SetState<String>,
    pub(crate) rules_filter: String,
    pub(crate) set_rules_filter: SetState<String>,
    pub(crate) connections_filter: String,
    pub(crate) set_connections_filter: SetState<String>,
    pub(crate) connection_selected: String,
    pub(crate) set_connection_selected: SetState<String>,
    pub(crate) action_busy: bool,
    pub(crate) set_action_busy: AsyncSetState<bool>,
    pub(crate) logs_status: String,
    pub(crate) set_logs_status: AsyncSetState<String>,
    pub(crate) logs_level_filter: String,
    pub(crate) set_logs_level_filter: SetState<String>,
    pub(crate) logs_paused: bool,
    pub(crate) set_logs_paused: SetState<bool>,
    pub(crate) logs_lines: Vec<String>,
    pub(crate) set_logs_lines: AsyncSetState<Vec<String>>,
    pub(crate) flows_status: String,
    pub(crate) set_flows_status: AsyncSetState<String>,
    pub(crate) flows_include_closed: bool,
    pub(crate) set_flows_include_closed: SetState<bool>,
    pub(crate) flows_filter: String,
    pub(crate) set_flows_filter: SetState<String>,
    pub(crate) flows_protocol: String,
    pub(crate) set_flows_protocol: SetState<String>,
    pub(crate) flows_sort: String,
    pub(crate) set_flows_sort: SetState<String>,
    pub(crate) flows_lines: Vec<String>,
    pub(crate) set_flows_lines: AsyncSetState<Vec<String>>,
}

const LOG_BUFFER_CAP: usize = 500;
const FLOW_BUFFER_CAP: usize = 300;

static LOG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static FLOW_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
/// Last full overview snapshot, so a `/ws/traffic` frame (which carries only
/// up/down rates) can patch the speed fields without re-fetching version,
/// configs, and memory. Seeded by `load_overview_snapshot` on navigation.
static OVERVIEW_CACHE: OnceLock<Mutex<OverviewSnapshot>> = OnceLock::new();

pub(crate) fn cache_overview_snapshot(snapshot: &OverviewSnapshot) {
    let mutex = OVERVIEW_CACHE.get_or_init(|| Mutex::new(OverviewSnapshot::empty()));
    if let Ok(mut guard) = mutex.lock() {
        *guard = snapshot.clone();
    }
}

/// Open the live subscription for `page`, returning the stop-sender that the
/// effect cleanup uses to tear it down. `None` for pages with no live data —
/// those run no background work at all. Each subscription owns a dedicated
/// thread + current-thread runtime holding the WebSocket open; signalling the
/// returned sender wakes the `select!` in the core streamer and ends the thread.
pub(crate) fn start_page_subscription(
    page: &str,
    state: &DashboardState,
    flows_include_closed: bool,
) -> Option<tokio::sync::watch::Sender<bool>> {
    match page {
        "logs" => Some(spawn_logs_subscription(state)),
        "flows" => Some(spawn_flows_subscription(state, flows_include_closed)),
        "connections" => Some(spawn_connections_subscription(state)),
        "overview" => Some(spawn_traffic_subscription(state)),
        _ => None,
    }
}

fn spawn_logs_subscription(state: &DashboardState) -> tokio::sync::watch::Sender<bool> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let set_status = state.set_logs_status.clone();
    let set_lines = state.set_logs_lines.clone();
    std::thread::spawn(move || {
        set_status.call("Connected".to_owned());
        let status_on_end = set_status.clone();
        let result = reactor_subscribe_messages("/ws/logs", move |frame| {
            let merged = with_buffer(&LOG_BUFFER, |buffer| {
                buffer.push(frame);
                *buffer = trim_tail(std::mem::take(buffer), LOG_BUFFER_CAP);
                buffer.clone()
            });
            set_lines.call(merged);
        }, stop_rx);
        report_subscription_end(&status_on_end, result);
    });
    stop_tx
}

fn spawn_flows_subscription(
    state: &DashboardState,
    include_closed: bool,
) -> tokio::sync::watch::Sender<bool> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let set_status = state.set_flows_status.clone();
    let set_lines = state.set_flows_lines.clone();
    let path = if include_closed {
        "/ws/flows?interval=1&top=50&include_closed=true"
    } else {
        "/ws/flows?interval=1&top=50&include_closed=false"
    };
    std::thread::spawn(move || {
        set_status.call("Connected".to_owned());
        let status_on_end = set_status.clone();
        let result = reactor_subscribe_messages(path, move |frame| {
            let merged = with_buffer(&FLOW_BUFFER, |buffer| {
                buffer.push(frame);
                *buffer = trim_tail(std::mem::take(buffer), FLOW_BUFFER_CAP);
                buffer.clone()
            });
            set_lines.call(merged);
        }, stop_rx);
        report_subscription_end(&status_on_end, result);
    });
    stop_tx
}

fn spawn_connections_subscription(state: &DashboardState) -> tokio::sync::watch::Sender<bool> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let set_connections = state.set_connections.clone();
    std::thread::spawn(move || {
        let result = reactor_subscribe_messages("/connections", move |frame| {
            if let Some(snapshot) = connections_snapshot_from_ws_frame(&frame) {
                set_connections.call(snapshot);
            }
        }, stop_rx);
        let _ = result;
    });
    stop_tx
}

fn spawn_traffic_subscription(state: &DashboardState) -> tokio::sync::watch::Sender<bool> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let set_overview = state.set_overview.clone();
    std::thread::spawn(move || {
        // Seed the cache with a full snapshot so traffic frames patch real
        // version/configs/memory values rather than an empty placeholder.
        let base = load_overview_snapshot();
        cache_overview_snapshot(&base);
        set_overview.call(base);
        let result = reactor_subscribe_messages("/ws/traffic", move |frame| {
            let Some((upload, download)) = traffic_speeds_from_ws_frame(&frame) else {
                return;
            };
            let mutex = OVERVIEW_CACHE.get_or_init(|| Mutex::new(OverviewSnapshot::empty()));
            let merged = match mutex.lock() {
                Ok(mut guard) => {
                    guard.upload_speed = upload;
                    guard.download_speed = download;
                    guard.clone()
                }
                Err(_) => return,
            };
            set_overview.call(merged);
        }, stop_rx);
        let _ = result;
    });
    stop_tx
}

fn report_subscription_end(
    set_status: &AsyncSetState<String>,
    result: std::result::Result<(), String>,
) {
    if result.is_err() {
        set_status.call("Disconnected".to_owned());
    }
}

pub(crate) fn refresh_logs_live(
    set_action_busy: AsyncSetState<bool>,
    set_logs_status: AsyncSetState<String>,
    set_logs_lines: AsyncSetState<Vec<String>>,
    logs_paused: bool,
) {
    if logs_paused {
        set_logs_status.call("Paused".to_owned());
        return;
    }
    set_action_busy.call(true);
    std::thread::spawn(move || {
        match reactor_stream_messages("/ws/logs", 120, 4) {
            Ok(messages) => {
                let mut merged = with_buffer(&LOG_BUFFER, |buffer| {
                    buffer.extend(messages);
                    *buffer = trim_tail(std::mem::take(buffer), LOG_BUFFER_CAP);
                    buffer.clone()
                });
                merged = trim_tail(merged, LOG_BUFFER_CAP);
                set_logs_lines.call(merged);
                set_logs_status.call("Connected".to_owned());
            }
            Err(_) => {
                set_logs_status.call("Disconnected".to_owned());
            }
        }
        set_action_busy.call(false);
    });
}

pub(crate) fn refresh_flows_live(
    set_action_busy: AsyncSetState<bool>,
    set_flows_status: AsyncSetState<String>,
    set_flows_lines: AsyncSetState<Vec<String>>,
    include_closed: bool,
) {
    set_action_busy.call(true);
    std::thread::spawn(move || {
        let path = if include_closed {
            "/ws/flows?interval=5&top=50&include_closed=true"
        } else {
            "/ws/flows?interval=5&top=50&include_closed=false"
        };
        match reactor_stream_messages(path, 60, 5) {
            Ok(messages) => {
                let merged = with_buffer(&FLOW_BUFFER, |buffer| {
                    buffer.extend(messages);
                    *buffer = trim_tail(std::mem::take(buffer), FLOW_BUFFER_CAP);
                    buffer.clone()
                });
                set_flows_lines.call(merged);
                set_flows_status.call("Connected".to_owned());
            }
            Err(_) => {
                set_flows_status.call("Disconnected".to_owned());
            }
        }
        set_action_busy.call(false);
    });
}

pub(crate) fn clear_logs_buffer(set_logs_lines: AsyncSetState<Vec<String>>) {
    with_buffer(&LOG_BUFFER, |buffer| {
        buffer.clear();
    });
    set_logs_lines.call(Vec::new());
}

fn with_buffer<T>(cell: &OnceLock<Mutex<Vec<String>>>, f: impl FnOnce(&mut Vec<String>) -> T) -> T {
    let mutex = cell.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

pub(crate) fn overview_page(
    state: &DashboardState,
    safety: &SafetyState,
    toasts: &ToastQueue,
) -> Element {
    vstack((
        overview_hero(state, toasts),
        metric_strip(state),
        section(
            "Runtime configuration",
            Some("Patch the live core or hot-reload from a different config path."),
            vstack((config_editor(state), config_actions(state, safety, toasts)))
                .spacing(spacing::MD_LG),
        ),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

fn overview_hero(state: &DashboardState, toasts: &ToastQueue) -> Element {
    let (status_label, status_color) = core_status_display(state);
    let detail = if state.overview.version != "Offline" && state.overview.version != "-" {
        state.overview.version.clone()
    } else {
        String::new()
    };
    let actions = hstack((
        button("Start")
            .icon(SymbolGlyph::Play)
            .on_click({
                let set_overview = state.set_overview.clone();
                let set_action_busy = state.set_action_busy.clone();
                let toasts = toasts.clone();
                move || {
                    spawn_action(
                        set_action_busy.clone(),
                        set_overview.clone(),
                        toasts.clone(),
                        "Core started",
                        reactor_start_core_text,
                    );
                }
            })
            .accent(),
        button("Stop")
            .icon(SymbolGlyph::Pause)
            .on_click({
                let set_overview = state.set_overview.clone();
                let set_action_busy = state.set_action_busy.clone();
                let toasts = toasts.clone();
                move || {
                    spawn_action(
                        set_action_busy.clone(),
                        set_overview.clone(),
                        toasts.clone(),
                        "Core stopped",
                        reactor_stop_core_text,
                    );
                }
            })
            .subtle(),
        button("Restart")
            .icon(SymbolGlyph::Redo)
            .on_click({
                let set_overview = state.set_overview.clone();
                let set_action_busy = state.set_action_busy.clone();
                let toasts = toasts.clone();
                move || {
                    spawn_action(
                        set_action_busy.clone(),
                        set_overview.clone(),
                        toasts.clone(),
                        "Core restarted",
                        reactor_restart_core_text,
                    );
                }
            })
            .subtle(),
    ))
    .spacing(spacing::SM);

    hero_card(status_label, status_color, detail, actions.into())
}

fn core_status_display(state: &DashboardState) -> (String, windows_reactor::ThemeRef) {
    if state.overview.lifecycle == "running" {
        ("Running".to_owned(), tokens::SystemSuccess)
    } else {
        ("Stopped".to_owned(), tokens::SecondaryText)
    }
}

fn metric_strip(state: &DashboardState) -> Element {
    metric_grid(vec![
        metric_card("Upload", &state.overview.upload_speed, "Live throughput"),
        metric_card("Download", &state.overview.download_speed, "Live throughput"),
        metric_card("Memory", &state.overview.memory, "Resident set"),
        metric_card("Connections", &state.overview.connections, "Active flows"),
        metric_card("Mixed Port", &state.overview.mixed_port, "Listening on"),
    ])
}

pub(crate) fn proxies_page(state: &DashboardState) -> Element {
    let groups: Vec<&ProxyRow> = state
        .proxies
        .rows
        .iter()
        .filter(|row| !row.member_names.is_empty())
        .collect();
    let statics: Vec<&ProxyRow> = state
        .proxies
        .rows
        .iter()
        .filter(|row| row.member_names.is_empty())
        .collect();

    let group_cards: Vec<Element> = if groups.is_empty() {
        vec![empty_row(&state.proxies.message)]
    } else {
        groups
            .iter()
            .map(|group| proxy_group_card(group, &state.set_proxies))
            .collect()
    };

    vstack((
        toolbar(hstack((
            text_block(format!(
                "{} groups · {} static",
                groups.len(),
                statics.len()
            ))
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        ))
        .spacing(spacing::SM)),
        section(
            "Proxy groups",
            Some("Tap a member to switch the active proxy for the group."),
            scroll_view(vstack(group_cards).spacing(spacing::SM + 2.0)).height(420.0),
        ),
        section(
            "Static proxies",
            Some("Direct, reject, and individual outbound proxies."),
            proxy_static_table(&statics),
        ),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

fn proxy_group_card(
    group: &ProxyRow,
    set_proxies: &AsyncSetState<ProxySnapshot>,
) -> Element {
    let chips: Vec<(String, Element)> = group
        .member_names
        .iter()
        .take(48)
        .map(|member| {
            let active = member == &group.current;
            let group_name = group.name.clone();
            let member_name = member.clone();
            let set_proxies = set_proxies.clone();
            let chip = button(member.clone()).on_click(move || {
                let group_name = group_name.clone();
                let member_name = member_name.clone();
                let set_proxies = set_proxies.clone();
                std::thread::spawn(move || {
                    reactor_select_proxy_text(&group_name, &member_name);
                    set_proxies.call(load_proxy_snapshot());
                });
            });
            let element: Element = if active { chip.accent() } else { chip.subtle() }.into();
            (member.clone(), element)
        })
        .collect();

    border(
        vstack((
            hstack((
                text_block(group.name.clone()).font_size(type_scale::BODY_STRONG).bold(),
                text_block(format!("[{}]", group.proxy_type))
                    .font_size(type_scale::CAPTION)
                    .foreground(tokens::SecondaryText),
                text_block(format!("→ {}", group.current))
                    .font_size(type_scale::CAPTION)
                    .foreground(tokens::SecondaryText),
                button("Test")
                    .icon(SymbolGlyph::Sync)
                    .on_click({
                        let group_name = group.name.clone();
                        let set_proxies = set_proxies.clone();
                        move || {
                            let group_name = group_name.clone();
                            let set_proxies = set_proxies.clone();
                            std::thread::spawn(move || {
                                reactor_group_delay_text(&group_name);
                                set_proxies.call(load_proxy_snapshot());
                            });
                        }
                    })
                    .subtle(),
            ))
            .spacing(spacing::SM),
            wrap_chips(chips),
        ))
        .spacing(spacing::SM),
    )
    .padding(Thickness::uniform(spacing::MD))
    .corner_radius(radius::MD)
    .background(tokens::CardBackground)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .into()
}

fn wrap_chips(chips: Vec<(String, Element)>) -> Element {
    const ROW_BUDGET: usize = 64;
    let mut rows: Vec<Vec<Element>> = Vec::new();
    let mut current: Vec<Element> = Vec::new();
    let mut current_len: usize = 0;
    for (label, chip) in chips {
        let weight = label.chars().count().max(8) + 4;
        if current_len + weight > ROW_BUDGET && !current.is_empty() {
            rows.push(std::mem::take(&mut current));
            current_len = 0;
        }
        current.push(chip);
        current_len += weight;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    let row_elements: Vec<Element> = rows
        .into_iter()
        .map(|row| hstack(row).spacing(spacing::XS + 2.0).into())
        .collect();
    vstack(row_elements).spacing(spacing::XS + 2.0).into()
}

fn proxy_static_table(rows: &[&ProxyRow]) -> Element {
    let mut out = vec![table_header_cols(&[
        ("Name", 220.0),
        ("Type", 120.0),
        ("Delay", 100.0),
    ])];
    if rows.is_empty() {
        out.push(empty_row("No static proxies."));
    } else {
        for proxy in rows.iter().take(60) {
            out.push(table_row_cols(vec![
                (proxy.name.clone(), 220.0),
                (proxy.proxy_type.clone(), 120.0),
                (proxy.delay.clone(), 100.0),
            ]));
        }
    }
    table_panel(out)
}

pub(crate) fn providers_page(state: &DashboardState) -> Element {
    let providers = proxy_provider_items(&state.providers);
    let proxies = proxy_items(&state.proxies);
    let rule_providers = rule_provider_items(&state.providers);
    let proxy_count = state
        .providers
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("proxy"))
        .count();
    let rule_count = state
        .providers
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("rule"))
        .count();
    vstack((
        toolbar(hstack((
            text_block(format!(
                "{proxy_count} proxy providers · {rule_count} rule providers"
            ))
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        ))
        .spacing(spacing::SM)),
        section(
            "Proxy providers",
            Some("Subscription sources that supply outbound proxy entries."),
            vstack((
                hstack((
                    ComboBox::new(providers.clone())
                        .placeholder("Provider")
                        .selected_index(choice_index_owned(&state.proxy_provider, &providers))
                        .on_selection_changed({
                            let set_proxy_provider = state.set_proxy_provider.clone();
                            move |index| set_choice_owned(index, &providers, &set_proxy_provider)
                        })
                        .width(240.0),
                    ComboBox::new(proxies.clone())
                        .placeholder("Proxy")
                        .selected_index(choice_index_owned(&state.provider_proxy, &proxies))
                        .on_selection_changed({
                            let set_provider_proxy = state.set_provider_proxy.clone();
                            move |index| set_choice_owned(index, &proxies, &set_provider_proxy)
                        })
                        .width(240.0),
                    button("Update")
                        .icon(SymbolGlyph::Download)
                        .on_click({
                            let proxy_provider = state.proxy_provider.clone();
                            let set_providers = state.set_providers.clone();
                            let set_proxies = state.set_proxies.clone();
                            move || {
                                let proxy_provider = proxy_provider.clone();
                                let set_providers = set_providers.clone();
                                let set_proxies = set_proxies.clone();
                                std::thread::spawn(move || {
                                    reactor_update_proxy_provider_text(&proxy_provider);
                                    set_providers.call(load_provider_snapshot());
                                    set_proxies.call(load_proxy_snapshot());
                                });
                            }
                        })
                        .subtle(),
                    button("Health check")
                        .icon(SymbolGlyph::Sync)
                        .on_click({
                            let proxy_provider = state.proxy_provider.clone();
                            let set_providers = state.set_providers.clone();
                            move || {
                                let proxy_provider = proxy_provider.clone();
                                let set_providers = set_providers.clone();
                                std::thread::spawn(move || {
                                    reactor_healthcheck_proxy_provider_text(&proxy_provider);
                                    set_providers.call(load_provider_snapshot());
                                });
                            }
                        })
                        .subtle(),
                    button("Test proxy")
                        .icon(SymbolGlyph::Sync)
                        .on_click({
                            let proxy_provider = state.proxy_provider.clone();
                            let provider_proxy = state.provider_proxy.clone();
                            let set_providers = state.set_providers.clone();
                            move || {
                                let proxy_provider = proxy_provider.clone();
                                let provider_proxy = provider_proxy.clone();
                                let set_providers = set_providers.clone();
                                std::thread::spawn(move || {
                                    reactor_healthcheck_provider_proxy_text(
                                        &proxy_provider,
                                        &provider_proxy,
                                    );
                                    set_providers.call(load_provider_snapshot());
                                });
                            }
                        })
                        .subtle(),
                ))
                .spacing(spacing::SM),
                proxy_provider_table(&state.providers),
            ))
            .spacing(spacing::SM + 2.0),
        ),
        section(
            "Rule providers",
            Some("External rule sets fetched and merged into the active config."),
            vstack((
                hstack((
                    ComboBox::new(rule_providers.clone())
                        .placeholder("Rule provider")
                        .selected_index(choice_index_owned(&state.rule_provider, &rule_providers))
                        .on_selection_changed({
                            let set_rule_provider = state.set_rule_provider.clone();
                            move |index| {
                                set_choice_owned(index, &rule_providers, &set_rule_provider)
                            }
                        })
                        .width(240.0),
                    button("Update")
                        .icon(SymbolGlyph::Download)
                        .on_click({
                            let rule_provider = state.rule_provider.clone();
                            let set_rules = state.set_rules.clone();
                            let set_providers = state.set_providers.clone();
                            move || {
                                let rule_provider = rule_provider.clone();
                                let set_rules = set_rules.clone();
                                let set_providers = set_providers.clone();
                                std::thread::spawn(move || {
                                    reactor_update_rule_provider_text(&rule_provider);
                                    set_providers.call(load_provider_snapshot());
                                    set_rules.call(load_rules_snapshot());
                                });
                            }
                        })
                        .subtle(),
                ))
                .spacing(spacing::SM),
                rule_provider_table(&state.providers),
            ))
            .spacing(spacing::SM + 2.0),
        ),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

fn proxy_provider_table(snapshot: &ProviderSnapshot) -> Element {
    let mut rows = vec![table_header_cols(&[
        ("Name", 200.0),
        ("Type", 100.0),
        ("Vehicle", 110.0),
        ("Count", 80.0),
        ("Updated", 200.0),
    ])];
    let visible: Vec<&_> = snapshot
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("proxy"))
        .collect();
    if visible.is_empty() {
        rows.push(empty_row("No proxy providers."));
    } else {
        for provider in visible.iter().take(60) {
            rows.push(table_row_cols(vec![
                (provider.name.clone(), 200.0),
                (provider.provider_type.clone(), 100.0),
                (provider.vehicle.clone(), 110.0),
                (provider.count.clone(), 80.0),
                (provider.updated_at.clone(), 200.0),
            ]));
        }
    }
    table_panel(rows)
}

fn rule_provider_table(snapshot: &ProviderSnapshot) -> Element {
    let mut rows = vec![table_header_cols(&[
        ("Name", 180.0),
        ("Type", 90.0),
        ("Vehicle", 100.0),
        ("Behavior", 100.0),
        ("Format", 90.0),
        ("Count", 80.0),
        ("Updated", 200.0),
    ])];
    let visible: Vec<&_> = snapshot
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("rule"))
        .collect();
    if visible.is_empty() {
        rows.push(empty_row("No rule providers."));
    } else {
        for provider in visible.iter().take(60) {
            rows.push(table_row_cols(vec![
                (provider.name.clone(), 180.0),
                (provider.provider_type.clone(), 90.0),
                (provider.vehicle.clone(), 100.0),
                (provider.behavior.clone(), 100.0),
                (provider.format.clone(), 90.0),
                (provider.count.clone(), 80.0),
                (provider.updated_at.clone(), 200.0),
            ]));
        }
    }
    table_panel(rows)
}

pub(crate) fn rules_page(state: &DashboardState) -> Element {
    vstack((
        toolbar(hstack((
            text_box(&state.rules_filter)
                .placeholder("Search by type, payload, or proxy…")
                .on_changed({
                    let set_rules_filter = state.set_rules_filter.clone();
                    move |value| set_rules_filter.call(value)
                })
                .width(360.0),
            text_block(format!("{} rules", state.rules.rows.len()))
                .font_size(type_scale::CAPTION)
                .foreground(tokens::SecondaryText),
        ))
        .spacing(spacing::SM)),
        rules_table(&state.rules, &state.rules_filter),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

pub(crate) fn connections_page(state: &DashboardState, safety: &SafetyState) -> Element {
    let visible: Vec<&_> = state
        .connections
        .rows
        .iter()
        .filter(|connection| {
            matches_filter(
                &state.connections_filter,
                &[
                    &connection.id,
                    &connection.host,
                    &connection.asn,
                    &connection.process,
                    &connection.network,
                    &connection.rule,
                    &connection.chains,
                ],
            )
        })
        .collect();
    let selected = if state.connection_selected.is_empty() {
        None
    } else {
        visible
            .iter()
            .find(|connection| connection.id == state.connection_selected)
            .map(|connection| (*connection).clone())
    };
    vstack((
        metric_grid(vec![
            metric_card("Upload", &state.connections.upload_total, "Cumulative"),
            metric_card("Download", &state.connections.download_total, "Cumulative"),
            metric_card(
                "Connections",
                &state.connections.rows.len().to_string(),
                "Active",
            ),
        ]),
        toolbar(hstack((
            button(confirm_label(
                safety,
                "close-all-connections",
                "Close all",
                "Confirm close all",
            ))
            .icon(SymbolGlyph::Delete)
            .on_click({
                let safety = safety.clone();
                let set_connections = state.set_connections.clone();
                let set_overview = state.set_overview.clone();
                move || {
                    guarded_action(&safety, "close-all-connections", || {
                        let set_connections = set_connections.clone();
                        let set_overview = set_overview.clone();
                        std::thread::spawn(move || {
                            reactor_close_all_connections_text();
                            set_connections.call(load_connections_snapshot());
                            set_overview.call(load_overview_snapshot());
                        });
                    });
                }
            })
            .subtle(),
            button("Refresh")
                .icon(SymbolGlyph::Sync)
                .on_click({
                    let set_connections = state.set_connections.clone();
                    move || {
                        let setter = set_connections.clone();
                        std::thread::spawn(move || setter.call(load_connections_snapshot()));
                    }
                })
                .subtle(),
            text_box(&state.connections_filter)
                .placeholder("Search host, ASN, process…")
                .on_changed({
                    let set_connections_filter = state.set_connections_filter.clone();
                    let set_connection_selected = state.set_connection_selected.clone();
                    move |value| {
                        set_connections_filter.call(value);
                        set_connection_selected.call(String::new());
                    }
                })
                .width(320.0),
        ))
        .spacing(spacing::SM)),
        connections_master_detail(state, safety, &visible, selected.as_ref()),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

fn connections_master_detail(
    state: &DashboardState,
    safety: &SafetyState,
    visible: &[&ConnectionRow],
    selected: Option<&ConnectionRow>,
) -> Element {
    if visible.is_empty() {
        return design_empty_state(
            "No active connections",
            if state.connections.message.is_empty() {
                "Once the core is running and traffic is flowing, sessions will appear here."
                    .to_owned()
            } else {
                state.connections.message.clone()
            },
            None,
        );
    }
    let capped: Vec<&ConnectionRow> = visible.iter().take(200).copied().collect();
    let labels: Vec<String> = capped
        .iter()
        .map(|c| {
            let host = if c.host.is_empty() {
                "—"
            } else {
                c.host.as_str()
            };
            format!("{host}  ·  {}  ·  {}", c.network, c.process)
        })
        .collect();
    let selected_index = capped
        .iter()
        .position(|c| c.id == state.connection_selected)
        .map(|index| index as i32)
        .unwrap_or(-1);
    let master = border(
        scroll_view(
            list_box()
                .items(labels)
                .selected_index(selected_index)
                .on_selection_changed({
                    let set_connection_selected = state.set_connection_selected.clone();
                    let ids: Vec<String> = capped.iter().map(|c| c.id.clone()).collect();
                    move |index: i32| {
                        let id = ids
                            .get(index.max(0) as usize)
                            .cloned()
                            .unwrap_or_default();
                        set_connection_selected.call(id);
                    }
                }),
        )
        .height(420.0),
    )
    .padding(Thickness::uniform(spacing::SM - 2.0))
    .corner_radius(radius::MD)
    .background(tokens::ControlFill)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .width(360.0);

    let detail: Element = match selected {
        None => design_empty_state(
            "Select a connection",
            "Pick a row from the list to inspect its host, rule chain, and live throughput.",
            None,
        ),
        Some(c) => connection_detail(c, state, safety),
    };
    hstack((master, detail)).spacing(spacing::MD).into()
}

fn connection_detail(
    c: &ConnectionRow,
    state: &DashboardState,
    safety: &SafetyState,
) -> Element {
    let host_label = if c.host.is_empty() {
        "Unknown host".to_owned()
    } else {
        c.host.clone()
    };
    section(
        "Connection detail",
        None,
        vstack((
            text_block(host_label).font_size(type_scale::TITLE).semibold(),
            text_block(format!(
                "{} · {} · {}",
                c.network,
                if c.process.is_empty() {
                    "no process"
                } else {
                    c.process.as_str()
                },
                if c.asn.is_empty() { "—" } else { c.asn.as_str() }
            ))
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
            detail_row("Rule", &c.rule),
            detail_row("Chain", &c.chains),
            detail_row("Upload", &c.upload),
            detail_row("Download", &c.download),
            detail_row("Duration", &c.duration),
            detail_row("ID", &c.id),
            hstack((
                button(confirm_label(
                    safety,
                    "close-one-connection",
                    "Close connection",
                    "Confirm close",
                ))
                .icon(SymbolGlyph::Cancel)
                .on_click({
                    let safety = safety.clone();
                    let id = c.id.clone();
                    let set_connections = state.set_connections.clone();
                    let set_overview = state.set_overview.clone();
                    let set_connection_selected = state.set_connection_selected.clone();
                    move || {
                        guarded_action(&safety, "close-one-connection", || {
                            let id = id.clone();
                            let set_connections = set_connections.clone();
                            let set_overview = set_overview.clone();
                            std::thread::spawn(move || {
                                if !id.is_empty() && is_plausible_connection_id(&id) {
                                    reactor_close_connection_text(&id);
                                }
                                set_connections.call(load_connections_snapshot());
                                set_overview.call(load_overview_snapshot());
                            });
                        });
                        set_connection_selected.call(String::new());
                    }
                })
                .subtle(),
            ))
            .spacing(spacing::SM),
        ))
        .spacing(spacing::SM + 2.0),
    )
}

fn detail_row(label: &str, value: &str) -> Element {
    let display = if value.is_empty() { "—" } else { value };
    hstack((
        text_block(label.to_owned())
            .font_size(type_scale::CAPTION)
            .bold()
            .foreground(tokens::SecondaryText)
            .width(96.0),
        selectable_text(display, 13.0),
    ))
    .spacing(spacing::SM)
    .into()
}

pub(crate) fn flows_page(state: &DashboardState) -> Element {
    let rows = visible_flow_rows(state);
    let (upload_total, download_total, connections_total) = flow_totals(&rows);
    let active_count = rows.iter().filter(|row| !row.closed).count();
    let closed_count = rows.iter().filter(|row| row.closed).count();
    let top_destination = rows
        .iter()
        .max_by(|left, right| left.bytes.cmp(&right.bytes))
        .map(|row| row.destination.clone())
        .unwrap_or_else(|| "-".to_owned());
    vstack((
        metric_grid(vec![
            metric_card("Top destination", &top_destination, "By bytes"),
            metric_card("Upload", &format_bytes_approx(upload_total), "Cumulative"),
            metric_card("Download", &format_bytes_approx(download_total), "Cumulative"),
            metric_card("Connections", &connections_total.to_string(), "Tracked"),
            metric_card("Active", &active_count.to_string(), "Flows"),
            metric_card("Closed", &closed_count.to_string(), "Flows"),
        ]),
        toolbar(hstack((
            status_pill(
                state.flows_status.as_str(),
                Tone::from_ok(state.flows_status == "Connected"),
            ),
            ToggleSwitch::new(state.flows_include_closed)
                .on_content("Include closed")
                .off_content("Active only")
                .on_changed({
                    let set_flows_include_closed = state.set_flows_include_closed.clone();
                    move |value| set_flows_include_closed.call(value)
                }),
            ComboBox::new(["bytes", "upload", "download", "connections"])
                .placeholder("Sort by")
                .selected_index(choice_index(
                    &state.flows_sort,
                    &["bytes", "upload", "download", "connections"],
                ))
                .on_selection_changed({
                    let set_flows_sort = state.set_flows_sort.clone();
                    move |index| {
                        set_choice(
                            index,
                            &["bytes", "upload", "download", "connections"],
                            &set_flows_sort,
                        )
                    }
                })
                .width(160.0),
            ComboBox::new(["all", "tcp", "udp", "quic"])
                .placeholder("Protocol")
                .selected_index(choice_index(
                    &state.flows_protocol,
                    &["all", "tcp", "udp", "quic"],
                ))
                .on_selection_changed({
                    let set_flows_protocol = state.set_flows_protocol.clone();
                    move |index| {
                        set_choice(index, &["all", "tcp", "udp", "quic"], &set_flows_protocol)
                    }
                })
                .width(120.0),
            text_box(&state.flows_filter)
                .placeholder("Search destination…")
                .on_changed({
                    let set_flows_filter = state.set_flows_filter.clone();
                    move |value| set_flows_filter.call(value)
                })
                .width(240.0),
            button("Refresh")
                .icon(SymbolGlyph::Sync)
                .on_click({
                    let set_action_busy = state.set_action_busy.clone();
                    let set_flows_status = state.set_flows_status.clone();
                    let set_flows_lines = state.set_flows_lines.clone();
                    let flows_include_closed = state.flows_include_closed;
                    move || refresh_flows_live(
                        set_action_busy.clone(),
                        set_flows_status.clone(),
                        set_flows_lines.clone(),
                        flows_include_closed,
                    )
                })
                .subtle(),
        ))
        .spacing(spacing::SM)),
        flow_graph_panel(&rows),
        flow_table(&rows),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

pub(crate) fn dns_page(state: &DashboardState) -> Element {
    vstack((
        toolbar(hstack((
            text_box(&state.dns_name)
                .placeholder("Hostname (example.com)")
                .on_changed({
                    let set_dns_name = state.set_dns_name.clone();
                    move |value| set_dns_name.call(value)
                })
                .width(320.0),
            ComboBox::new(["A", "AAAA", "CNAME", "MX", "TXT", "NS", "SOA"])
                .placeholder("Type")
                .selected_index(choice_index(
                    &state.dns_type,
                    &["A", "AAAA", "CNAME", "MX", "TXT", "NS", "SOA"],
                ))
                .on_selection_changed({
                    let set_dns_type = state.set_dns_type.clone();
                    move |index| {
                        set_choice(
                            index,
                            &["A", "AAAA", "CNAME", "MX", "TXT", "NS", "SOA"],
                            &set_dns_type,
                        )
                    }
                })
                .width(120.0),
            button("Query")
                .icon(SymbolGlyph::Find)
                .on_click({
                    let set_dns = state.set_dns.clone();
                    let dns_name = state.dns_name.clone();
                    let dns_type = state.dns_type.clone();
                    move || {
                        let set_dns = set_dns.clone();
                        let dns_name = dns_name.clone();
                        let dns_type = dns_type.clone();
                        std::thread::spawn(move || {
                            set_dns.call(load_dns_snapshot(&dns_name, &dns_type));
                        });
                    }
                })
                .accent(),
        ))
        .spacing(spacing::SM)),
        section(
            "Resolved answers",
            Some("Results from the controller's DNS resolver."),
            dns_table(&state.dns),
        ),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

pub(crate) fn logs_page(state: &DashboardState) -> Element {
    let visible_logs = filtered_logs(&state.logs_lines, &state.logs_level_filter);
    vstack((
        toolbar(hstack((
            status_pill(
                state.logs_status.as_str(),
                Tone::from_ok(
                    state.logs_status == "Connected" || state.logs_status == "Paused",
                ),
            ),
            ComboBox::new(["all", "debug", "info", "warning", "error"])
                .placeholder("Level")
                .selected_index(choice_index(
                    &state.logs_level_filter,
                    &["all", "debug", "info", "warning", "error"],
                ))
                .on_selection_changed({
                    let set_logs_level_filter = state.set_logs_level_filter.clone();
                    move |index| {
                        set_choice(
                            index,
                            &["all", "debug", "info", "warning", "error"],
                            &set_logs_level_filter,
                        )
                    }
                })
                .width(140.0),
            ToggleSwitch::new(state.logs_paused)
                .on_content("Paused")
                .off_content("Live")
                .on_changed({
                    let set_logs_paused = state.set_logs_paused.clone();
                    move |value| set_logs_paused.call(value)
                }),
            button("Refresh")
                .icon(SymbolGlyph::Sync)
                .on_click({
                    let set_action_busy = state.set_action_busy.clone();
                    let set_logs_status = state.set_logs_status.clone();
                    let set_logs_lines = state.set_logs_lines.clone();
                    let logs_paused = state.logs_paused;
                    move || refresh_logs_live(
                        set_action_busy.clone(),
                        set_logs_status.clone(),
                        set_logs_lines.clone(),
                        logs_paused,
                    )
                })
                .subtle(),
            button("Clear")
                .icon(SymbolGlyph::Delete)
                .on_click({
                    let set_logs_lines = state.set_logs_lines.clone();
                    move || clear_logs_buffer(set_logs_lines.clone())
                })
                .subtle(),
        ))
        .spacing(spacing::SM + 2.0)),
        log_table(&visible_logs),
    ))
    .spacing(spacing::MD_LG)
    .into()
}

fn rules_table(snapshot: &RulesSnapshot, filter: &str) -> Element {
    if snapshot.rows.is_empty() {
        return design_empty_state(
            "No rules loaded",
            if snapshot.message.is_empty() {
                "Activate a profile to see its routing rules here.".to_owned()
            } else {
                snapshot.message.clone()
            },
            None,
        );
    }
    let mut rows = vec![table_header(vec!["Type", "Payload", "Proxy"])];
    let visible = snapshot
        .rows
        .iter()
        .filter(|rule| matches_filter(filter, &[&rule.rule_type, &rule.payload, &rule.proxy]))
        .collect::<Vec<_>>();
    if visible.is_empty() {
        rows.push(empty_row("No rules match the current search."));
    } else {
        for rule in visible.iter().take(120) {
            rows.push(table_row(vec![
                rule.rule_type.clone(),
                rule.payload.clone(),
                rule.proxy.clone(),
            ]));
        }
        rows.push(count_row(visible.len(), snapshot.rows.len(), 120));
    }
    table_panel(rows)
}

fn dns_table(snapshot: &DnsSnapshot) -> Element {
    let mut rows = vec![table_header(vec!["Name", "Type", "TTL", "Data"])];
    if snapshot.rows.is_empty() {
        rows.push(empty_row(&snapshot.message));
    } else {
        for answer in &snapshot.rows {
            rows.push(table_row(vec![
                answer.name.clone(),
                answer.record_type.clone(),
                answer.ttl.clone(),
                answer.data.clone(),
            ]));
        }
    }
    table_panel(rows)
}

fn table_panel(rows: Vec<Element>) -> Element {
    border(scroll_view(vstack(rows).spacing(2.0)).height(360.0))
        .padding(Thickness::uniform(spacing::SM + 2.0))
        .corner_radius(radius::MD)
        .background(tokens::ControlFill)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(HAIRLINE))
        .into()
}

fn table_header(columns: Vec<&'static str>) -> Element {
    hstack(
        columns
            .into_iter()
            .map(|column| {
                text_block(column)
                    .font_size(type_scale::CAPTION)
                    .bold()
                    .foreground(tokens::SecondaryText)
                    .width(130.0)
                    .into()
            })
            .collect::<Vec<Element>>(),
    )
    .spacing(spacing::SM)
    .into()
}

fn table_header_cols(columns: &[(&'static str, f64)]) -> Element {
    hstack(
        columns
            .iter()
            .map(|(label, width)| {
                text_block(*label)
                    .font_size(type_scale::CAPTION)
                    .bold()
                    .foreground(tokens::SecondaryText)
                    .width(*width)
                    .into()
            })
            .collect::<Vec<Element>>(),
    )
    .spacing(spacing::SM)
    .into()
}

fn table_row(columns: Vec<String>) -> Element {
    hstack(
        columns
            .into_iter()
            .map(|column| cell_text(&column, 130.0))
            .collect::<Vec<Element>>(),
    )
    .spacing(spacing::SM)
    .into()
}

fn table_row_cols(columns: Vec<(String, f64)>) -> Element {
    hstack(
        columns
            .into_iter()
            .map(|(value, width)| cell_text(&value, width))
            .collect::<Vec<Element>>(),
    )
    .spacing(spacing::SM)
    .into()
}

/// A fixed-width table cell. When the value is too long to fit it is truncated
/// with an ellipsis, and the full value is attached as a hover tooltip so no
/// information is lost — the old behavior dropped the tail unrecoverably.
fn cell_text(value: &str, width: f64) -> Element {
    let (display, truncated) = truncate_cell(value);
    let cell = text_block(display).font_size(type_scale::CAPTION).width(width);
    if truncated {
        cell.tooltip(value.to_owned()).into()
    } else {
        cell.into()
    }
}

fn empty_row(message: &str) -> Element {
    border(
        text_block(message.to_owned())
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
    )
    .padding(Thickness::uniform(spacing::MD))
    .corner_radius(radius::MD)
    .background(tokens::SubtleFill)
    .into()
}

fn count_row(visible: usize, total: usize, cap: usize) -> Element {
    let capped = visible.min(cap);
    empty_row(&format!(
        "Showing {capped} of {visible} matching rows ({total} total)."
    ))
}

fn panel(title: &'static str, content: impl Into<Element>) -> Element {
    section(title, None, content)
}

fn matches_filter(filter: &str, values: &[&str]) -> bool {
    let filter = filter.trim().to_ascii_lowercase();
    filter.is_empty()
        || values
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(&filter))
}

fn proxy_items(snapshot: &ProxySnapshot) -> Vec<String> {
    let mut items = snapshot
        .rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();
    if items.is_empty() {
        items.push("DIRECT".to_owned());
    }
    items
}

fn proxy_provider_items(snapshot: &ProviderSnapshot) -> Vec<String> {
    let mut items = snapshot
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("proxy"))
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();
    if items.is_empty() {
        items.push("default".to_owned());
    }
    items
}

fn rule_provider_items(snapshot: &ProviderSnapshot) -> Vec<String> {
    let mut items = snapshot
        .rows
        .iter()
        .filter(|row| row.scope.eq_ignore_ascii_case("rule"))
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();
    if items.is_empty() {
        items.push("default".to_owned());
    }
    items
}

fn is_plausible_connection_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn choice_index_owned(value: &str, choices: &[String]) -> i32 {
    choices
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(value.trim()))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

fn set_choice_owned(index: i32, choices: &[String], setter: &SetState<String>) {
    if let Some(choice) = choices.get(index.max(0) as usize) {
        setter.call(choice.clone());
    }
}

/// Truncate a cell value to a fixed character budget, returning the display
/// string and whether truncation occurred (so callers can attach a tooltip
/// carrying the full value).
fn truncate_cell(value: &str) -> (String, bool) {
    const MAX_CHARS: usize = 34;
    if value.chars().count() > MAX_CHARS {
        let mut output = value.chars().take(MAX_CHARS).collect::<String>();
        output.push_str("...");
        (output, true)
    } else {
        (value.to_owned(), false)
    }
}

fn choice_index(value: &str, choices: &[&str]) -> i32 {
    choices
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(value.trim()))
        .map(|index| index as i32)
        .unwrap_or(-1)
}

fn set_choice(index: i32, choices: &[&str], setter: &SetState<String>) {
    if let Some(choice) = choices.get(index.max(0) as usize) {
        setter.call((*choice).to_owned());
    }
}

fn effective_str<'a>(local: &'a str, snapshot: &'a str) -> &'a str {
    if local.is_empty() {
        let trimmed = snapshot.trim();
        if trimmed == "-" || trimmed.is_empty() { "" } else { snapshot }
    } else {
        local
    }
}

/// Like [`effective_str`] but falls back to `default` when neither the local
/// edit nor the live snapshot has a usable value. Without this, the config
/// controls render blank/unselected whenever the core is offline (the snapshot
/// is `"-"`), so a fresh app shows empty Mode/Log/Port fields instead of the
/// values the core actually ships with.
fn effective_or_default(local: &str, snapshot: &str, default: &str) -> String {
    let effective = effective_str(local, snapshot);
    if effective.is_empty() {
        default.to_owned()
    } else {
        effective.to_owned()
    }
}

fn config_editor(state: &DashboardState) -> Element {
    let mode = effective_or_default(&state.config_mode, &state.overview.mode, "rule");
    let log_level =
        effective_or_default(&state.config_log_level, &state.overview.log_level, "info");
    let mixed_port =
        effective_or_default(&state.config_mixed_port, &state.overview.mixed_port, "7890");
    let bind_address = state.config_bind_address.clone();
    vstack((
        hstack((
            RadioButtons::new(["Rule", "Global", "Direct"])
                .header("Mode")
                .selected_index(choice_index(&mode, &["rule", "global", "direct"]))
                .max_columns(3)
                .on_selection_changed({
                    let set_config_mode = state.set_config_mode.clone();
                    move |index| set_choice(index, &["rule", "global", "direct"], &set_config_mode)
                })
                .width(360.0),
            ComboBox::new(["info", "warning", "error", "debug", "trace", "silent"])
                .header("Log level")
                .placeholder("Log level")
                .selected_index(choice_index(
                    &log_level,
                    &["info", "warning", "error", "debug", "trace", "silent"],
                ))
                .on_selection_changed({
                    let set_config_log_level = state.set_config_log_level.clone();
                    move |index| {
                        set_choice(
                            index,
                            &["info", "warning", "error", "debug", "trace", "silent"],
                            &set_config_log_level,
                        )
                    }
                })
                .width(180.0),
        ))
        .spacing(spacing::MD_LG),
        hstack((
            text_box(&mixed_port)
                .header("Mixed port")
                .placeholder("7890")
                .on_changed({
                    let set_config_mixed_port = state.set_config_mixed_port.clone();
                    move |value| set_config_mixed_port.call(value)
                })
                .width(140.0),
            text_box(&bind_address)
                .header("Bind address")
                .placeholder("*")
                .on_changed({
                    let set_config_bind_address = state.set_config_bind_address.clone();
                    move |value| set_config_bind_address.call(value)
                })
                .width(180.0),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::SM + 2.0)
    .into()
}


fn parse_on_off(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Some(true),
        "off" | "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn config_actions(state: &DashboardState, safety: &SafetyState, toasts: &ToastQueue) -> Element {
    let snapshot_allow_lan = parse_on_off(&state.overview.allow_lan);
    let snapshot_ipv6 = parse_on_off(&state.overview.ipv6);
    let effective_allow_lan = if state.config_allow_lan_dirty {
        state.config_allow_lan
    } else {
        snapshot_allow_lan.unwrap_or(state.config_allow_lan)
    };
    let effective_ipv6 = if state.config_ipv6_dirty {
        state.config_ipv6
    } else {
        snapshot_ipv6.unwrap_or(state.config_ipv6)
    };
    let patch_mode = effective_str(&state.config_mode, &state.overview.mode).to_owned();
    let patch_log_level = effective_str(&state.config_log_level, &state.overview.log_level).to_owned();
    let patch_mixed_port = effective_str(&state.config_mixed_port, &state.overview.mixed_port).to_owned();
    let patch_bind_address = state.config_bind_address.clone();
    vstack((
        hstack((
            ToggleSwitch::new(effective_allow_lan)
                .header("Allow LAN")
                .on_content("On")
                .off_content("Off")
                .on_changed({
                    let set_config_allow_lan = state.set_config_allow_lan.clone();
                    let set_config_allow_lan_dirty = state.set_config_allow_lan_dirty.clone();
                    move |value| {
                        set_config_allow_lan.call(value);
                        set_config_allow_lan_dirty.call(true);
                    }
                }),
            ToggleSwitch::new(effective_ipv6)
                .header("IPv6")
                .on_content("On")
                .off_content("Off")
                .on_changed({
                    let set_config_ipv6 = state.set_config_ipv6.clone();
                    let set_config_ipv6_dirty = state.set_config_ipv6_dirty.clone();
                    move |value| {
                        set_config_ipv6.call(value);
                        set_config_ipv6_dirty.call(true);
                    }
                }),
            button(confirm_label(
                safety,
                "patch-runtime-config",
                "Patch Configs",
                "Confirm Patch",
            ))
            .on_click({
                let safety = safety.clone();
                let mode = patch_mode;
                let log_level = patch_log_level;
                let mixed_port = patch_mixed_port;
                let bind_address = patch_bind_address;
                let allow_lan = effective_allow_lan;
                let ipv6 = effective_ipv6;
                let set_overview = state.set_overview.clone();
                let set_action_busy = state.set_action_busy.clone();
                let toasts = toasts.clone();
                move || {
                    guarded_action(&safety, "patch-runtime-config", || {
                        spawn_action(
                            set_action_busy.clone(),
                            set_overview.clone(),
                            toasts.clone(),
                            "Configs patched",
                            {
                                let mode = mode.clone();
                                let log_level = log_level.clone();
                                let mixed_port = mixed_port.clone();
                                let bind_address = bind_address.clone();
                                move || {
                                    reactor_patch_configs_text(
                                        &mode,
                                        &log_level,
                                        &mixed_port,
                                        &bind_address,
                                        allow_lan,
                                        ipv6,
                                    )
                                }
                            },
                        );
                    });
                }
            })
            .accent(),
        ))
        .spacing(spacing::MD),
        hstack((
            text_box(&state.config_reload_path)
                .header("Reload path")
                .placeholder(r"C:\path\config.yaml")
                .on_changed({
                    let set_config_reload_path = state.set_config_reload_path.clone();
                    move |value| set_config_reload_path.call(value)
                })
                .width(360.0),
            button("Browse")
                .icon(SymbolGlyph::Find)
                .on_click({
                    let set_config_reload_path = state.set_config_reload_path.clone();
                    move || {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("YAML", &["yaml", "yml"])
                            .pick_file()
                        {
                            set_config_reload_path.call(path.display().to_string());
                        }
                    }
                })
                .subtle(),
            button(confirm_label(
                safety,
                "reload-runtime-config",
                "Reload Configs",
                "Confirm Reload",
            ))
            .on_click({
                let safety = safety.clone();
                let config_reload_path = state.config_reload_path.clone();
                let set_overview = state.set_overview.clone();
                let set_action_busy = state.set_action_busy.clone();
                let toasts = toasts.clone();
                move || {
                    guarded_action(&safety, "reload-runtime-config", || {
                        let path = config_reload_path.clone();
                        spawn_action(
                            set_action_busy.clone(),
                            set_overview.clone(),
                            toasts.clone(),
                            "Configs reloaded",
                            move || reactor_reload_configs_text(&path),
                        );
                    });
                }
            })
            .subtle(),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::SM)
    .into()
}

#[derive(Clone)]
struct FlowRowView {
    destination: String,
    protocol: String,
    connections: u64,
    upload: u64,
    download: u64,
    bytes: u64,
    closed: bool,
}

fn visible_flow_rows(state: &DashboardState) -> Vec<FlowRowView> {
    let mut rows = parse_flow_rows(&state.flows_lines)
        .into_iter()
        .filter(|row| {
            let protocol_ok = state.flows_protocol.eq_ignore_ascii_case("all")
                || row
                    .protocol
                    .eq_ignore_ascii_case(state.flows_protocol.trim());
            let closed_ok = state.flows_include_closed || !row.closed;
            let filter_ok = state.flows_filter.trim().is_empty()
                || row
                    .destination
                    .to_ascii_lowercase()
                    .contains(&state.flows_filter.trim().to_ascii_lowercase());
            protocol_ok && closed_ok && filter_ok
        })
        .collect::<Vec<_>>();

    match state.flows_sort.trim().to_ascii_lowercase().as_str() {
        "upload" => rows.sort_by(|left, right| right.upload.cmp(&left.upload)),
        "download" => rows.sort_by(|left, right| right.download.cmp(&left.download)),
        "connections" => rows.sort_by(|left, right| right.connections.cmp(&left.connections)),
        _ => rows.sort_by(|left, right| right.bytes.cmp(&left.bytes)),
    }
    rows
}

fn parse_flow_rows(lines: &[String]) -> Vec<FlowRowView> {
    let mut rows = Vec::new();
    for line in lines {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(array) = value.get("flows").and_then(serde_json::Value::as_array) {
                for item in array {
                    rows.push(flow_row_from_json(item));
                }
                continue;
            }
            if let Some(array) = value.as_array() {
                for item in array {
                    rows.push(flow_row_from_json(item));
                }
                continue;
            }
            rows.push(flow_row_from_json(&value));
        }
    }
    rows
}

fn flow_row_from_json(value: &serde_json::Value) -> FlowRowView {
    let destination = value
        .get("destination")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("host").and_then(serde_json::Value::as_str))
        .unwrap_or("-")
        .to_owned();
    let protocol = value
        .get("protocol")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("network").and_then(serde_json::Value::as_str))
        .unwrap_or("-")
        .to_owned();
    let connections = value
        .get("connections")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("count").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    let upload = value
        .get("upload")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("up").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    let download = value
        .get("download")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("down").and_then(serde_json::Value::as_u64))
        .unwrap_or(0);
    let bytes = value
        .get("bytes")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(upload.saturating_add(download));
    let closed = value
        .get("closed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    FlowRowView {
        destination,
        protocol,
        connections,
        upload,
        download,
        bytes,
        closed,
    }
}

fn flow_totals(rows: &[FlowRowView]) -> (u64, u64, u64) {
    rows.iter()
        .fold((0_u64, 0_u64, 0_u64), |(up, down, conns), row| {
            (
                up.saturating_add(row.upload),
                down.saturating_add(row.download),
                conns.saturating_add(row.connections),
            )
        })
}

fn flow_table(rows: &[FlowRowView]) -> Element {
    let mut table_rows = vec![table_header(vec![
        "Destination",
        "Protocol",
        "Connections",
        "Upload",
        "Download",
        "Bytes",
        "Closed",
    ])];
    if rows.is_empty() {
        table_rows.push(empty_row("No flow rows match the current filters."));
    } else {
        for row in rows.iter().take(120) {
            table_rows.push(table_row(vec![
                row.destination.clone(),
                row.protocol.clone(),
                row.connections.to_string(),
                format_bytes_approx(row.upload),
                format_bytes_approx(row.download),
                format_bytes_approx(row.bytes),
                if row.closed {
                    "yes".to_owned()
                } else {
                    "no".to_owned()
                },
            ]));
        }
        table_rows.push(count_row(rows.len(), rows.len(), 120));
    }
    table_panel(table_rows)
}

fn flow_graph_panel(rows: &[FlowRowView]) -> Element {
    let top = rows.iter().take(8).collect::<Vec<_>>();
    if top.is_empty() {
        return panel(
            "Top destinations by traffic",
            design_empty_state(
                "No flow data yet",
                "Refresh flows while the core is running to chart the busiest destinations.",
                None,
            ),
        );
    }
    let max_bytes = top.iter().map(|row| row.bytes).max().unwrap_or(1).max(1);
    let bars: Vec<Element> = top
        .into_iter()
        .map(|row| flow_bar_row(row, max_bytes))
        .collect();
    panel(
        "Top destinations by traffic",
        scroll_view(vstack(bars).spacing(spacing::SM)).height(200.0),
    )
}

/// One horizontal bar: destination label, a proportional filled track, and the
/// byte total. The track is a two-cell star grid so the fill width is a true
/// fraction of the busiest row instead of repeated `#` characters.
fn flow_bar_row(row: &FlowRowView, max_bytes: u64) -> Element {
    let fraction = (row.bytes as f64 / max_bytes as f64).clamp(0.0, 1.0);
    // Star weights must stay positive so an empty row still shows a sliver.
    let filled = (fraction * 1000.0).round().max(1.0);
    let rest = (1000.0 - filled).max(0.0);
    let fill: Element = border(Element::Empty)
        .height(14.0)
        // Half the bar height → fully-rounded ends; a geometric value, not a
        // card radius, so it stays a literal rather than a `radius::` token.
        .corner_radius(7.0)
        .background(tokens::Accent)
        .grid_column(0)
        .into();
    let track_children: Vec<Element> = if rest > 0.0 {
        vec![fill, border(Element::Empty).grid_column(1).into()]
    } else {
        vec![fill]
    };
    let track: Element = grid(track_children)
        .columns(if rest > 0.0 {
            vec![GridLength::Star(filled), GridLength::Star(rest)]
        } else {
            vec![GridLength::Star(filled)]
        })
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .into();

    let (dest_display, dest_truncated) = truncate_cell(&row.destination);
    let dest_label = text_block(dest_display)
        .font_size(type_scale::CAPTION)
        .grid_column(0);
    let dest_label: Element = if dest_truncated {
        dest_label.tooltip(row.destination.clone()).into()
    } else {
        dest_label.into()
    };
    grid(vec![
        dest_label,
        track.grid_column(1),
        text_block(format_bytes_approx(row.bytes))
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText)
            .horizontal_alignment(HorizontalAlignment::Right)
            .grid_column(2)
            .into(),
    ])
    .columns([
        GridLength::Pixel(150.0),
        GridLength::STAR,
        GridLength::Pixel(90.0),
    ])
    .column_spacing(spacing::MD)
    .into()
}

fn filtered_logs(lines: &[String], level: &str) -> Vec<String> {
    let level = level.trim().to_ascii_lowercase();
    if level.is_empty() || level == "all" {
        return lines.to_vec();
    }
    lines
        .iter()
        .filter(|line| line.to_ascii_lowercase().contains(&level))
        .cloned()
        .collect::<Vec<_>>()
}

fn parse_log_line(line: &str) -> (String, String) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
        let level = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("-")
            .to_owned();
        let payload = value
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(line)
            .to_owned();
        (level, payload)
    } else {
        ("-".to_owned(), line.to_owned())
    }
}

fn level_tone(level: &str) -> (ThemeRef, &'static str) {
    let lower = level.to_ascii_lowercase();
    match lower.as_str() {
        "info" => (tokens::SystemSuccess, "INFO"),
        "warning" | "warn" => (tokens::SystemAttention, "WARN"),
        "error" | "err" => (tokens::SystemCritical, "ERROR"),
        "debug" => (tokens::SecondaryText, "DEBUG"),
        "trace" => (tokens::TertiaryText, "TRACE"),
        _ => (tokens::TertiaryText, "—"),
    }
}

fn log_row(line: &str) -> Element {
    let (level, message) = parse_log_line(line);
    let (color, label) = level_tone(&level);
    let stripe = border(Element::Empty)
        .background(color.clone())
        .width(3.0);
    let body = border(
        hstack((
            text_block(label.to_owned())
                .font_size(type_scale::CAPTION)
                .bold()
                .foreground(color)
                .width(56.0),
            selectable_text(&message, 13.0),
        ))
        .spacing(spacing::SM + 2.0),
    )
    .padding(Thickness::xy(spacing::SM + 2.0, spacing::SM - 2.0))
    .background(tokens::SubtleFill);
    hstack((stripe, body)).spacing(0.0).into()
}

fn log_table(lines: &[String]) -> Element {
    if lines.is_empty() {
        return design_empty_state(
            "Logs are quiet",
            "No log entries collected yet. Start the core or refresh logs to begin streaming.",
            None,
        );
    }
    let rows: Vec<Element> = lines
        .iter()
        .rev()
        .take(300)
        .map(|line| log_row(line))
        .collect();
    border(scroll_view(vstack(rows).spacing(2.0)).height(420.0))
        .padding(Thickness::uniform(spacing::SM))
        .corner_radius(radius::MD)
        .background(tokens::ControlFill)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(HAIRLINE))
        .into()
}

fn trim_tail(mut lines: Vec<String>, max_items: usize) -> Vec<String> {
    if lines.len() > max_items {
        let drain = lines.len() - max_items;
        lines.drain(0..drain);
    }
    lines
}

fn format_bytes_approx(value: u64) -> String {
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

#[cfg(test)]
mod tests {
    use super::{
        choice_index, effective_or_default, effective_str, filtered_logs, flow_totals,
        format_bytes_approx, is_plausible_connection_id, looks_like_failure, matches_filter,
        parse_flow_rows, parse_on_off, trim_tail, truncate_cell,
    };

    #[test]
    fn trim_tail_keeps_recent_entries() {
        let items = vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
            "four".to_owned(),
        ];
        let trimmed = trim_tail(items, 2);
        assert_eq!(trimmed, vec!["three".to_owned(), "four".to_owned()]);
    }

    #[test]
    fn trim_tail_is_noop_when_under_cap() {
        let items = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(trim_tail(items.clone(), 5), items);
    }

    #[test]
    fn filtered_logs_respects_level() {
        let logs = vec![
            "[info] connected".to_owned(),
            "[warning] timeout".to_owned(),
            "[error] down".to_owned(),
        ];
        let warnings = filtered_logs(&logs, "warning");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("warning"));
    }

    #[test]
    fn filtered_logs_all_returns_everything() {
        let logs = vec!["[info] a".to_owned(), "[error] b".to_owned()];
        assert_eq!(filtered_logs(&logs, "all").len(), 2);
        assert_eq!(filtered_logs(&logs, "").len(), 2);
    }

    #[test]
    fn parse_flow_rows_reads_json_entries() {
        let lines = vec![
            r#"{"destination":"example.com","protocol":"tcp","connections":2,"upload":1024,"download":2048}"#
                .to_owned(),
        ];
        let rows = parse_flow_rows(&lines);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].destination, "example.com");
        assert_eq!(rows[0].connections, 2);
    }

    #[test]
    fn parse_flow_rows_unwraps_flows_envelope_and_defaults_bytes() {
        // A frame wrapping rows under "flows"; bytes omitted → upload+download.
        let lines = vec![
            r#"{"flows":[{"host":"a.com","network":"udp","up":10,"down":20}]}"#.to_owned(),
        ];
        let rows = parse_flow_rows(&lines);
        assert_eq!(rows.len(), 1);
        // Falls back to host/network aliases.
        assert_eq!(rows[0].destination, "a.com");
        assert_eq!(rows[0].protocol, "udp");
        assert_eq!(rows[0].bytes, 30);
    }

    #[test]
    fn flow_totals_sum_upload_download_connections() {
        let lines = vec![
            r#"{"destination":"a","upload":100,"download":200,"connections":1}"#.to_owned(),
            r#"{"destination":"b","upload":50,"download":25,"connections":3}"#.to_owned(),
        ];
        let rows = parse_flow_rows(&lines);
        assert_eq!(flow_totals(&rows), (150, 225, 4));
    }

    #[test]
    fn looks_like_failure_ignores_json_body_after_first_line() {
        // Success line, then a body that happens to contain "error".
        assert!(!looks_like_failure(
            "select proxy: ok\n{\n  \"note\": \"error-handler proxy\"\n}"
        ));
        // Real failures are on the first line.
        assert!(looks_like_failure("select proxy failed: no such group"));
        assert!(looks_like_failure("could not reach controller"));
        assert!(looks_like_failure("permission denied"));
    }

    #[test]
    fn effective_str_prefers_local_then_meaningful_snapshot() {
        // Local edit wins.
        assert_eq!(effective_str("rule", "global"), "rule");
        // Empty local falls back to snapshot...
        assert_eq!(effective_str("", "global"), "global");
        // ...unless the snapshot is a placeholder dash or blank.
        assert_eq!(effective_str("", "-"), "");
        assert_eq!(effective_str("", "   "), "");
    }

    #[test]
    fn effective_or_default_fills_blank_controls() {
        // Local edit always wins.
        assert_eq!(effective_or_default("global", "rule", "rule"), "global");
        // Live snapshot used when no local edit.
        assert_eq!(effective_or_default("", "direct", "rule"), "direct");
        // Both blank/offline → the shipped default, never an empty control.
        assert_eq!(effective_or_default("", "-", "rule"), "rule");
        assert_eq!(effective_or_default("", "", "info"), "info");
    }

    #[test]
    fn choice_index_matches_case_insensitively_or_minus_one() {
        let choices = ["rule", "global", "direct"];
        assert_eq!(choice_index("global", &choices), 1);
        assert_eq!(choice_index("GLOBAL", &choices), 1);
        assert_eq!(choice_index(" direct ", &choices), 2);
        assert_eq!(choice_index("missing", &choices), -1);
    }

    #[test]
    fn plausible_connection_id_rejects_unsafe_strings() {
        assert!(is_plausible_connection_id("abc-123_DEF"));
        assert!(!is_plausible_connection_id(""));
        assert!(!is_plausible_connection_id("has space"));
        assert!(!is_plausible_connection_id("path/../traversal"));
    }

    #[test]
    fn matches_filter_is_case_insensitive_and_empty_matches_all() {
        assert!(matches_filter("", &["anything"]));
        assert!(matches_filter("HOST", &["myhost.com", "tcp"]));
        assert!(matches_filter("  tcp ", &["myhost.com", "TCP"]));
        assert!(!matches_filter("zzz", &["myhost.com", "tcp"]));
    }

    #[test]
    fn parse_on_off_accepts_known_tokens() {
        assert_eq!(parse_on_off("on"), Some(true));
        assert_eq!(parse_on_off("TRUE"), Some(true));
        assert_eq!(parse_on_off("1"), Some(true));
        assert_eq!(parse_on_off("off"), Some(false));
        assert_eq!(parse_on_off("no"), Some(false));
        assert_eq!(parse_on_off("maybe"), None);
    }

    #[test]
    fn format_bytes_approx_scales_units() {
        assert_eq!(format_bytes_approx(512), "512 B");
        assert_eq!(format_bytes_approx(1024), "1.0 KiB");
        assert_eq!(format_bytes_approx(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn truncate_cell_flags_and_ellipsizes_only_when_over_budget() {
        // Short value: unchanged, not flagged → no tooltip attached.
        let (display, truncated) = truncate_cell("short");
        assert_eq!(display, "short");
        assert!(!truncated);
        // Long value: ellipsized and flagged so the caller adds a tooltip.
        let long = "x".repeat(50);
        let (display, truncated) = truncate_cell(&long);
        assert!(truncated);
        assert!(display.ends_with("..."));
        assert_eq!(display.chars().count(), 34 + 3);
    }
}
