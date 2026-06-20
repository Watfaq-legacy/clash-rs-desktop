use windows_reactor::*;

use std::time::Duration;

use clash_rs_desktop_core::{TunLaunchPolicy, UpdateChannel};

use crate::{
    controller_models::{
        ConnectionsSnapshot, DnsSnapshot, OverviewSnapshot, ProviderSnapshot, ProxySnapshot,
        RulesSnapshot, load_connections_snapshot, load_overview_snapshot, load_provider_snapshot,
        load_proxy_snapshot, load_rules_snapshot,
    },
    profile_actions::{
        ProfileSummary, normalize_profile_index, reactor_profile_override_form,
        reactor_profile_summaries,
    },
    settings_actions::{
        reactor_controller_inputs, reactor_load_global_override_text, reactor_settings_inputs,
    },
    views::{
        dashboard::{
            DashboardState, connections_page, dns_page, flows_page, logs_page, overview_page,
            providers_page, proxies_page, rules_page, start_page_subscription,
        },
        design::{Toast, ToastQueue, busy_strip, spacing, toast_stack},
        output::SafetyState,
        profiles::{ProfilesState, profiles_section},
        service::{ServiceState, service_toolbar},
        settings::{SettingsState, settings_section},
    },
};

pub(crate) fn app(_cx: &mut RenderCx) -> Element {
    let (selected_page, set_selected_page) = _cx.use_state("overview".to_owned());
    let dashboard_state = dashboard_state(_cx);
    let profiles_state = profiles_state(_cx);
    let settings_state = settings_state(_cx);
    let service_state = service_state(_cx);
    let (armed_action, set_armed_action) = _cx.use_state(String::new());
    let safety_state = SafetyState {
        armed_action,
        set_armed_action,
    };
    let toast_queue = toast_queue(_cx);

    configure_toast_purge(_cx, &toast_queue);
    configure_live_subscription(_cx, &selected_page, &dashboard_state);
    configure_service_status_probe(_cx, &service_state);

    NavigationView::new(
        navigation_items(),
        app_workspace(
            &selected_page,
            &dashboard_state,
            &profiles_state,
            &settings_state,
            &service_state,
            &safety_state,
            &toast_queue,
            set_selected_page.clone(),
        ),
    )
    .selected_tag(selected_page.clone())
    .on_selection_changed({
        let set_selected_page = set_selected_page.clone();
        let dashboard_state = dashboard_state.clone();
        let service_state = service_state.clone();
        move |tag: String| {
            refresh_dashboard_page(&tag, &dashboard_state);
            if tag == "service" {
                let setter = service_state.set_status.clone();
                std::thread::spawn(move || {
                    setter.call(crate::service_actions::reactor_service_status_text());
                });
            }
            set_selected_page.call(tag);
        }
    })
    .pane_title("clash-rs")
    .header(page_title(&selected_page))
    .pane_display_mode(NavViewPaneDisplayMode::Left)
    .pane_open(true)
    .settings_visible(false)
    .back_button_visible(false)
    .auto_suggest_placeholder("Jump to a page")
    .auto_suggest_items([
        "Overview",
        "Proxies",
        "Providers",
        "Rules",
        "Connections",
        "Flows",
        "DNS",
        "Logs",
        "Profiles",
        "Settings",
        "Service",
    ])
    .on_search_query_submitted({
        let set_selected_page = set_selected_page.clone();
        let dashboard_state = dashboard_state.clone();
        let service_state = service_state.clone();
        move |query| {
            if let Some(tag) = page_tag_for_query(&query) {
                refresh_dashboard_page(&tag, &dashboard_state);
                if tag == "service" {
                    let setter = service_state.set_status.clone();
                    std::thread::spawn(move || {
                        setter.call(crate::service_actions::reactor_service_status_text());
                    });
                }
                set_selected_page.call(tag);
            }
        }
    })
    .into()
}

/// Hold a persistent WebSocket subscription open for the active page's live
/// data and tear it down when the page (or a relevant toggle) changes. Keyed
/// on the inputs that determine the subscription target so navigating away,
/// pausing logs, or toggling closed-flow inclusion re-establishes it. Pages
/// without live data run no subscription — and therefore no background work.
fn configure_live_subscription(
    cx: &mut RenderCx,
    selected_page: &str,
    dashboard_state: &DashboardState,
) {
    let selected_page = selected_page.to_owned();
    let logs_paused = dashboard_state.logs_paused;
    let flows_include_closed = dashboard_state.flows_include_closed;
    let state = dashboard_state.clone();

    cx.use_effect_with_cleanup(
        (selected_page.clone(), logs_paused, flows_include_closed),
        move || {
            let stop = start_page_subscription(&selected_page, &state, flows_include_closed);
            stop.map(|stop_tx| {
                let cleanup: Box<dyn FnOnce()> = Box::new(move || {
                    let _ = stop_tx.send(true);
                });
                cleanup
            })
        },
    );
}

/// Fetch the Windows service status once on app launch so the user immediately
/// sees whether the helper service is installed/running — and the not-installed
/// nav banner can appear — without having to open the Service page first. Keyed
/// on a constant so it runs exactly once for the app's lifetime. The fetch
/// touches the SCM and the service pipe (blocking), so it runs on a worker
/// thread and marshals the result back via the async setter.
fn configure_service_status_probe(cx: &mut RenderCx, service_state: &ServiceState) {
    let set_status = service_state.set_status.clone();
    cx.use_effect((), move || {
        std::thread::spawn(move || {
            set_status.call(crate::service_actions::reactor_service_status_text());
        });
    });
}

fn app_workspace(
    selected_page: &str,
    dashboard_state: &DashboardState,
    profiles_state: &ProfilesState,
    settings_state: &SettingsState,
    service_state: &ServiceState,
    safety_state: &SafetyState,
    toast_queue: &ToastQueue,
    set_selected_page: SetState<String>,
) -> Element {
    // These siblings are diffed positionally, and the toast slot drops to
    // `Element::Empty` when there are no toasts — which shifts the page into the
    // toast's old index. Since every page is itself a `StackPanel`, an unkeyed
    // diff then *morphs* the toast panel into the page (and one page into the
    // next on navigation) in place, bleeding stale sections through. Stable keys
    // — the page keyed by its tag — force identity matching so each page cleanly
    // unmounts and remounts instead.
    scroll_view(
        vstack((
            busy_strip(dashboard_state.action_busy).with_key("workspace-busy"),
            service_nav_banner(selected_page, service_state, &set_selected_page)
                .with_key("workspace-service-banner"),
            toast_stack(toast_queue).with_key("workspace-toasts"),
            page_content(
                selected_page,
                dashboard_state,
                profiles_state,
                settings_state,
                service_state,
                safety_state,
                toast_queue,
                set_selected_page,
            )
            .with_key(format!("workspace-page-{selected_page}")),
        ))
        .spacing(spacing::LG + 2.0)
        .padding(Thickness::uniform(spacing::XXL)),
    )
    .vertical_scroll_bar_visibility(ScrollViewScrollBarVisibility::Visible)
    .into()
}

/// An app-wide banner shown on any page *except* Service when the helper service
/// is not installed, with a button that jumps to the Service page so the user
/// can install it. This is the discoverability bridge for the first-run flow:
/// the status is probed on launch, so a fresh install surfaces the call to
/// action immediately instead of the user having to find the Service page.
/// Renders nothing once the service exists, on the Service page itself, or
/// before the first status arrives.
fn service_nav_banner(
    selected_page: &str,
    service_state: &ServiceState,
    set_selected_page: &SetState<String>,
) -> Element {
    use crate::service_actions::service_status_indicates_not_installed;

    if selected_page == "service" || !service_status_indicates_not_installed(&service_state.status) {
        return Element::Empty;
    }

    let go_to_service = {
        let set_selected_page = set_selected_page.clone();
        let service_state = service_state.clone();
        move || {
            // Mirror the nav handler's side effect: refresh service status as we
            // land on the page so the install controls reflect live state.
            let setter = service_state.set_status.clone();
            std::thread::spawn(move || {
                setter.call(crate::service_actions::reactor_service_status_text());
            });
            set_selected_page.call("service".to_owned());
        }
    };

    border(
        grid((
            vstack((
                text_block("Windows helper service not installed")
                    .font_size(16.0)
                    .semibold(),
                text_block(
                    "TUN mode and elevated core launches route through the helper service. \
                     Install it to enable them.",
                )
                .font_size(12.0)
                .foreground(tokens::SecondaryText),
            ))
            .spacing(2.0)
            .grid_column(0),
            button("Open Service page")
                .icon(SymbolGlyph::Setting)
                .on_click(go_to_service)
                .accent()
                .horizontal_alignment(HorizontalAlignment::Right)
                .grid_column(1),
        ))
        .columns([GridLength::STAR, GridLength::Auto])
        .column_spacing(spacing::LG)
        .horizontal_alignment(HorizontalAlignment::Stretch),
    )
    .padding(Thickness::uniform(spacing::LG))
    .corner_radius(8.0)
    .background(tokens::CardBackground)
    .border_brush(tokens::SystemAttention)
    .border_thickness(Thickness::uniform(1.0))
    .into()
}

fn page_content(
    selected_page: &str,
    dashboard_state: &DashboardState,
    profiles_state: &ProfilesState,
    settings_state: &SettingsState,
    service_state: &ServiceState,
    safety_state: &SafetyState,
    toast_queue: &ToastQueue,
    _set_selected_page: SetState<String>,
) -> Element {
    match selected_page {
        "overview" => overview_page(dashboard_state, safety_state, toast_queue),
        "proxies" => proxies_page(dashboard_state),
        "providers" => providers_page(dashboard_state),
        "rules" => rules_page(dashboard_state),
        "connections" => connections_page(dashboard_state, safety_state),
        "flows" => flows_page(dashboard_state),
        "dns" => dns_page(dashboard_state),
        "logs" => logs_page(dashboard_state),
        "profiles" => profiles_section(profiles_state, safety_state, toast_queue),
        "settings" => settings_section(settings_state, safety_state, toast_queue),
        "service" => service_toolbar(service_state, safety_state, toast_queue),
        _ => overview_page(dashboard_state, safety_state, toast_queue),
    }
}

fn service_state(cx: &mut RenderCx) -> ServiceState {
    let (status, set_status) =
        cx.use_async_state("Check Status to inspect the Windows service.".to_owned());
    ServiceState { status, set_status }
}

fn toast_queue(cx: &mut RenderCx) -> ToastQueue {
    let (toasts, set_toasts) = cx.use_async_state(Vec::<Toast>::new());
    let (next_id_value, next_id) = cx.use_async_state(0_u64);
    ToastQueue {
        toasts,
        set_toasts,
        next_id,
        next_id_value,
    }
}

fn configure_toast_purge(cx: &mut RenderCx, queue: &ToastQueue) {
    let queue = queue.clone();
    let has_toasts = !queue.toasts.is_empty();
    cx.use_effect_with_cleanup(has_toasts, move || {
        if !has_toasts {
            return None::<Box<dyn FnOnce()>>;
        }
        let queue = queue.clone();
        let timer = DispatcherTimer::new(Duration::from_millis(500), move || {
            queue.purge_expired();
        })
        .ok();
        timer.map(|t| {
            let boxed: Box<dyn FnOnce()> = Box::new(move || drop(t));
            boxed
        })
    });
}

fn navigation_items() -> Vec<NavViewItem> {
    vec![
        NavViewItem::header("Dashboard"),
        NavViewItem::new("Overview")
            .tag("overview")
            .icon(SymbolGlyph::Home),
        NavViewItem::new("Proxies")
            .tag("proxies")
            .icon(SymbolGlyph::World),
        NavViewItem::new("Providers")
            .tag("providers")
            .icon(SymbolGlyph::Download),
        NavViewItem::new("Rules")
            .tag("rules")
            .icon(SymbolGlyph::Flag),
        NavViewItem::new("Connections")
            .tag("connections")
            .icon(SymbolGlyph::People),
        NavViewItem::new("Flows")
            .tag("flows")
            .icon(SymbolGlyph::Sync),
        NavViewItem::new("DNS").tag("dns").icon(SymbolGlyph::Find),
        NavViewItem::new("Logs").tag("logs").icon(SymbolGlyph::Edit),
        NavViewItem::header("Management"),
        NavViewItem::new("Profiles")
            .tag("profiles")
            .icon(SymbolGlyph::Copy),
        NavViewItem::new("Settings")
            .tag("settings")
            .icon(SymbolGlyph::Setting),
        NavViewItem::new("Service")
            .tag("service")
            .icon(SymbolGlyph::Play),
    ]
}

fn page_title(tag: &str) -> String {
    match tag {
        "overview" => "Overview",
        "proxies" => "Proxies",
        "providers" => "Providers",
        "rules" => "Rules",
        "connections" => "Connections",
        "flows" => "Flows",
        "dns" => "DNS",
        "logs" => "Logs",
        "profiles" => "Profiles",
        "settings" => "Settings",
        "service" => "Windows Service",
        _ => "Overview",
    }
    .to_owned()
}

fn page_tag_for_query(query: &str) -> Option<String> {
    let normalized = query.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "overview" => Some("overview"),
        "proxies" | "proxy" => Some("proxies"),
        "providers" | "provider" => Some("providers"),
        "rules" | "rule" => Some("rules"),
        "connections" | "connection" => Some("connections"),
        "flows" | "flow" => Some("flows"),
        "dns" => Some("dns"),
        "logs" | "log" => Some("logs"),
        "profiles" | "profile" => Some("profiles"),
        "settings" | "setting" => Some("settings"),
        "service" => Some("service"),
        _ => None,
    }
    .map(str::to_owned)
}

fn dashboard_state(cx: &mut RenderCx) -> DashboardState {
    let (overview, set_overview) = cx.use_async_state(OverviewSnapshot::empty());
    let (proxies, set_proxies) = cx.use_async_state(ProxySnapshot::empty());
    let (providers, set_providers) = cx.use_async_state(ProviderSnapshot::empty());
    let (rules, set_rules) = cx.use_async_state(RulesSnapshot::empty());
    let (connections, set_connections) = cx.use_async_state(ConnectionsSnapshot::empty());
    let (dns, set_dns) = cx.use_async_state(DnsSnapshot::empty());
    let (dns_name, set_dns_name) = cx.use_state(String::new());
    let (dns_type, set_dns_type) = cx.use_state("A".to_owned());
    let (config_mode, set_config_mode) = cx.use_state(String::new());
    let (config_log_level, set_config_log_level) = cx.use_state(String::new());
    let (config_mixed_port, set_config_mixed_port) = cx.use_state(String::new());
    let (config_bind_address, set_config_bind_address) = cx.use_state(String::new());
    let (config_allow_lan, set_config_allow_lan) = cx.use_state(false);
    let (config_allow_lan_dirty, set_config_allow_lan_dirty) = cx.use_state(false);
    let (config_ipv6, set_config_ipv6) = cx.use_state(false);
    let (config_ipv6_dirty, set_config_ipv6_dirty) = cx.use_state(false);
    let (config_reload_path, set_config_reload_path) = cx.use_state(String::new());
    let (proxy_provider, set_proxy_provider) = cx.use_state(String::new());
    let (provider_proxy, set_provider_proxy) = cx.use_state(String::new());
    let (rule_provider, set_rule_provider) = cx.use_state(String::new());
    let (rules_filter, set_rules_filter) = cx.use_state(String::new());
    let (connections_filter, set_connections_filter) = cx.use_state(String::new());
    let (connection_selected, set_connection_selected) = cx.use_state(String::new());
    let (action_busy, set_action_busy) = cx.use_async_state(false);
    let (logs_status, set_logs_status) = cx.use_async_state("Disconnected".to_owned());
    let (logs_level_filter, set_logs_level_filter) = cx.use_state("all".to_owned());
    let (logs_paused, set_logs_paused) = cx.use_state(false);
    let (logs_lines, set_logs_lines) = cx.use_async_state(Vec::<String>::new());
    let (flows_status, set_flows_status) = cx.use_async_state("Idle".to_owned());
    let (flows_include_closed, set_flows_include_closed) = cx.use_state(false);
    let (flows_filter, set_flows_filter) = cx.use_state(String::new());
    let (flows_protocol, set_flows_protocol) = cx.use_state("all".to_owned());
    let (flows_sort, set_flows_sort) = cx.use_state("bytes".to_owned());
    let (flows_lines, set_flows_lines) = cx.use_async_state(Vec::<String>::new());

    DashboardState {
        overview,
        set_overview,
        proxies,
        set_proxies,
        providers,
        set_providers,
        rules,
        set_rules,
        connections,
        set_connections,
        dns,
        set_dns,
        dns_name,
        set_dns_name,
        dns_type,
        set_dns_type,
        config_mode,
        set_config_mode,
        config_log_level,
        set_config_log_level,
        config_mixed_port,
        set_config_mixed_port,
        config_bind_address,
        set_config_bind_address,
        config_allow_lan,
        set_config_allow_lan,
        config_allow_lan_dirty,
        set_config_allow_lan_dirty,
        config_ipv6,
        set_config_ipv6,
        config_ipv6_dirty,
        set_config_ipv6_dirty,
        config_reload_path,
        set_config_reload_path,
        proxy_provider,
        set_proxy_provider,
        provider_proxy,
        set_provider_proxy,
        rule_provider,
        set_rule_provider,
        rules_filter,
        set_rules_filter,
        connections_filter,
        set_connections_filter,
        connection_selected,
        set_connection_selected,
        action_busy,
        set_action_busy,
        logs_status,
        set_logs_status,
        logs_level_filter,
        set_logs_level_filter,
        logs_paused,
        set_logs_paused,
        logs_lines,
        set_logs_lines,
        flows_status,
        set_flows_status,
        flows_include_closed,
        set_flows_include_closed,
        flows_filter,
        set_flows_filter,
        flows_protocol,
        set_flows_protocol,
        flows_sort,
        set_flows_sort,
        flows_lines,
        set_flows_lines,
    }
}

fn refresh_dashboard_page(tag: &str, state: &DashboardState) {
    match tag {
        "overview" => {
            let setter = state.set_overview.clone();
            std::thread::spawn(move || setter.call(load_overview_snapshot()));
        }
        "proxies" => {
            let setter = state.set_proxies.clone();
            std::thread::spawn(move || setter.call(load_proxy_snapshot()));
        }
        "providers" => {
            let setter = state.set_providers.clone();
            std::thread::spawn(move || setter.call(load_provider_snapshot()));
        }
        "rules" => {
            let setter = state.set_rules.clone();
            std::thread::spawn(move || setter.call(load_rules_snapshot()));
        }
        "connections" => {
            let setter = state.set_connections.clone();
            std::thread::spawn(move || setter.call(load_connections_snapshot()));
        }
        _ => {}
    }
}

fn profiles_state(cx: &mut RenderCx) -> ProfilesState {
    let (profile_index, set_profile_index) = cx.use_state(0_i32);
    let (import_name, set_import_name) = cx.use_state(String::new());
    let (import_path, set_import_path) = cx.use_state(String::new());
    let (remote_name, set_remote_name) = cx.use_state(String::new());
    let (remote_url, set_remote_url) = cx.use_state(String::new());
    let profiles = reactor_profile_summaries().unwrap_or_else(|_| Vec::new());
    let selected_profile_index = normalize_profile_index(profile_index, profiles.len());
    let initial_override = reactor_profile_override_form(selected_profile_index);
    let (profile_override, set_profile_override) = cx.use_async_state(initial_override.raw_yaml);
    let (override_mode, set_override_mode) = cx.use_async_state(initial_override.mode);
    let (override_log_level, set_override_log_level) = cx.use_async_state(initial_override.log_level);
    let (override_mixed_port, set_override_mixed_port) = cx.use_async_state(initial_override.mixed_port);
    let (override_allow_lan, set_override_allow_lan) = cx.use_async_state(initial_override.allow_lan);
    let (override_ipv6, set_override_ipv6) = cx.use_async_state(initial_override.ipv6);
    let (profiles_version, set_profiles_version) = cx.use_async_state(0_u32);
    let (global_override, set_global_override) = cx.use_async_state(reactor_load_global_override_text());
    let profile_items = if profiles.is_empty() {
        vec!["No profiles configured".to_owned()]
    } else {
        profiles
            .iter()
            .map(ProfileSummary::display_label)
            .collect::<Vec<_>>()
    };

    ProfilesState {
        profile_index,
        set_profile_index,
        selected_profile_index,
        profile_items,
        import_name,
        set_import_name,
        import_path,
        set_import_path,
        remote_name,
        set_remote_name,
        remote_url,
        set_remote_url,
        profile_override,
        set_profile_override,
        override_mode,
        set_override_mode,
        override_log_level,
        set_override_log_level,
        override_mixed_port,
        set_override_mixed_port,
        override_allow_lan,
        set_override_allow_lan,
        override_ipv6,
        set_override_ipv6,
        profiles_version,
        set_profiles_version,
        global_override,
        set_global_override,
    }
}

fn settings_state(cx: &mut RenderCx) -> SettingsState {
    let initial_settings = reactor_settings_inputs();
    let controller_inputs = reactor_controller_inputs();
    let (update_channel, set_update_channel) =
        cx.use_state(update_channel_index(&initial_settings.update_channel));
    let (auto_check, set_auto_check) = cx.use_state(initial_settings.auto_check);
    let (tun_policy, set_tun_policy) = cx.use_state(tun_policy_index(&initial_settings.tun_policy));
    let (proxy_host, set_proxy_host) = cx.use_state(initial_settings.proxy_host);
    let (proxy_port, set_proxy_port) = cx.use_state(initial_settings.proxy_port);
    let (proxy_bypass, set_proxy_bypass) = cx.use_state(initial_settings.proxy_bypass);
    let (controller_endpoint, set_controller_endpoint) = cx.use_state(controller_inputs.endpoint);
    let (controller_secret, set_controller_secret) = cx.use_state(controller_inputs.secret);
    let (core_status, set_core_status) =
        cx.use_async_state(crate::update_actions::reactor_core_status_text());

    SettingsState {
        update_channel,
        set_update_channel,
        auto_check,
        set_auto_check,
        core_status,
        set_core_status,
        tun_policy,
        set_tun_policy,
        proxy_host,
        set_proxy_host,
        proxy_port,
        set_proxy_port,
        proxy_bypass,
        set_proxy_bypass,
        controller_endpoint,
        set_controller_endpoint,
        controller_secret,
        set_controller_secret,
    }
}

fn update_channel_index(channel: &UpdateChannel) -> i32 {
    match channel {
        UpdateChannel::Stable => 0,
        UpdateChannel::Preview => 1,
    }
}

fn tun_policy_index(policy: &TunLaunchPolicy) -> i32 {
    match policy {
        TunLaunchPolicy::RequireService => 0,
        TunLaunchPolicy::DenyUntilServiceInstalled => 1,
    }
}
