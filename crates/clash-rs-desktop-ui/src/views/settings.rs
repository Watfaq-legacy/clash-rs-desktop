use clash_rs_desktop_core::{TunLaunchPolicy, UpdateChannel};
use windows_reactor::*;

use crate::settings_actions::{
    reactor_apply_system_proxy_text, reactor_restore_system_proxy_text,
    reactor_save_controller_endpoint_text, reactor_save_proxy_target_text,
    reactor_set_tun_policy_text, reactor_set_update_channel_text,
    reactor_toggle_auto_check_text,
};
use crate::views::dashboard::report_action;
use crate::views::design::{ToastQueue, section, settings_row, spacing};
use crate::views::output::{SafetyState, confirm_label, guarded_action};

#[derive(Clone)]
pub(crate) struct SettingsState {
    pub(crate) update_channel: i32,
    pub(crate) set_update_channel: SetState<i32>,
    pub(crate) auto_check: bool,
    pub(crate) set_auto_check: SetState<bool>,
    pub(crate) tun_policy: i32,
    pub(crate) set_tun_policy: SetState<i32>,
    pub(crate) proxy_host: String,
    pub(crate) set_proxy_host: SetState<String>,
    pub(crate) proxy_port: String,
    pub(crate) set_proxy_port: SetState<String>,
    pub(crate) proxy_bypass: String,
    pub(crate) set_proxy_bypass: SetState<String>,
    pub(crate) controller_endpoint: String,
    pub(crate) set_controller_endpoint: SetState<String>,
    pub(crate) controller_secret: String,
    pub(crate) set_controller_secret: SetState<String>,
}

pub(crate) fn settings_section(
    state: &SettingsState,
    safety: &SafetyState,
    toasts: &ToastQueue,
) -> Element {
    vstack((
        section(
            "Updates",
            Some("Choose how the bundled core is fetched and applied."),
            update_settings_rows(state),
        ),
        section(
            "TUN launch policy",
            Some("Decide whether the UI may launch TUN mode without the helper service."),
            tun_policy_row(state),
        ),
        section(
            "Windows system proxy",
            Some("Apply or restore the global proxy settings used by Windows apps."),
            proxy_target_row(state, safety, toasts),
        ),
        section(
            "Controller endpoint",
            Some("Pipe or HTTP endpoint the UI uses to talk to the running core."),
            controller_endpoint_row(state, toasts),
        ),
    ))
    .spacing(spacing::MD + 2.0)
    .into()
}

fn update_settings_rows(state: &SettingsState) -> Element {
    vstack((
        settings_row(
            "Update channel",
            "Stable for daily use; Preview for early builds.",
            RadioButtons::new(["Stable", "Preview"])
                .selected_index(state.update_channel)
                .max_columns(2)
                .on_selection_changed({
                    let set_update_channel = state.set_update_channel.clone();
                    move |index| {
                        set_update_channel.call(index);
                        std::thread::spawn(move || {
                            reactor_set_update_channel_text(update_channel(index));
                        });
                    }
                })
                .width(220.0)
                .into(),
        ),
        settings_row(
            "Automatic update checks",
            "Periodically poll for newer core builds in the background.",
            ToggleSwitch::new(state.auto_check)
                .on_content("On")
                .off_content("Off")
                .on_changed({
                    let set_auto_check = state.set_auto_check.clone();
                    move |enabled| {
                        set_auto_check.call(enabled);
                        std::thread::spawn(|| {
                            reactor_toggle_auto_check_text();
                        });
                    }
                })
                .into(),
        ),
    ))
    .spacing(spacing::SM)
    .into()
}

fn proxy_target_row(
    state: &SettingsState,
    safety: &SafetyState,
    toasts: &ToastQueue,
) -> Element {
    vstack((
        hstack((
            text_box(&state.proxy_host)
                .header("Host")
                .placeholder("127.0.0.1")
                .on_changed({
                    let set_proxy_host = state.set_proxy_host.clone();
                    move |value| set_proxy_host.call(value)
                })
                .width(220.0),
            text_box(&state.proxy_port)
                .header("Port")
                .placeholder("7890")
                .on_changed({
                    let set_proxy_port = state.set_proxy_port.clone();
                    move |value| set_proxy_port.call(value)
                })
                .width(120.0),
        ))
        .spacing(spacing::SM + 2.0),
        text_box(&state.proxy_bypass)
            .header("Bypass list")
            .placeholder("localhost;127.*;<local>")
            .on_changed({
                let set_proxy_bypass = state.set_proxy_bypass.clone();
                move |value| set_proxy_bypass.call(value)
            })
            .width(520.0),
        hstack((
            button("Save Target")
                .icon(SymbolGlyph::Save)
                .on_click({
                    let proxy_host = state.proxy_host.clone();
                    let proxy_port = state.proxy_port.clone();
                    let proxy_bypass = state.proxy_bypass.clone();
                    let toasts = toasts.clone();
                    move || {
                        let proxy_host = proxy_host.clone();
                        let proxy_port = proxy_port.clone();
                        let proxy_bypass = proxy_bypass.clone();
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let result = reactor_save_proxy_target_text(&proxy_host, &proxy_port, &proxy_bypass);
                            report_action(&toasts, "System-proxy target saved", &result);
                        });
                    }
                })
                .subtle(),
            button(confirm_label(
                safety,
                "apply-system-proxy",
                "Apply",
                "Confirm Apply",
            ))
            .icon(SymbolGlyph::Accept)
            .on_click({
                let safety = safety.clone();
                let toasts = toasts.clone();
                move || {
                    let toasts = toasts.clone();
                    guarded_action(&safety, "apply-system-proxy", move || {
                        std::thread::spawn(move || {
                            let result = reactor_apply_system_proxy_text();
                            report_action(&toasts, "System proxy applied", &result);
                        });
                    })
                }
            })
            .accent(),
            button(confirm_label(
                safety,
                "restore-system-proxy",
                "Restore",
                "Confirm Restore",
            ))
            .icon(SymbolGlyph::Undo)
            .on_click({
                let safety = safety.clone();
                let toasts = toasts.clone();
                move || {
                    let toasts = toasts.clone();
                    guarded_action(&safety, "restore-system-proxy", move || {
                        std::thread::spawn(move || {
                            let result = reactor_restore_system_proxy_text();
                            report_action(&toasts, "System proxy restored", &result);
                        });
                    })
                }
            })
            .subtle(),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::MD)
    .into()
}

fn tun_policy_row(state: &SettingsState) -> Element {
    RadioButtons::new(["Use service for TUN", "Block TUN without service"])
        .selected_index(state.tun_policy)
        .on_selection_changed({
            let set_tun_policy = state.set_tun_policy.clone();
            move |index| {
                set_tun_policy.call(index);
                std::thread::spawn(move || {
                    reactor_set_tun_policy_text(tun_policy(index));
                });
            }
        })
        .into()
}

fn controller_endpoint_row(state: &SettingsState, toasts: &ToastQueue) -> Element {
    vstack((
        text_box(&state.controller_endpoint)
            .header("Endpoint")
            .placeholder(r"\\.\pipe\clash-rs-desktop-core-local or http://127.0.0.1:9090")
            .on_changed({
                let set_controller_endpoint = state.set_controller_endpoint.clone();
                move |value| set_controller_endpoint.call(value)
            })
            .width(560.0),
        text_box(&state.controller_secret)
            .header("Secret")
            .placeholder("Optional bearer token")
            .on_changed({
                let set_controller_secret = state.set_controller_secret.clone();
                move |value| set_controller_secret.call(value)
            })
            .width(320.0),
        button("Save Endpoint")
            .icon(SymbolGlyph::Save)
            .on_click({
                let endpoint = state.controller_endpoint.clone();
                let secret = state.controller_secret.clone();
                let toasts = toasts.clone();
                move || {
                    let endpoint = endpoint.clone();
                    let secret = secret.clone();
                    let toasts = toasts.clone();
                    std::thread::spawn(move || {
                        let result = reactor_save_controller_endpoint_text(&endpoint, &secret);
                        report_action(&toasts, "Controller endpoint saved", &result);
                    });
                }
            })
            .accent(),
    ))
    .spacing(spacing::SM + 2.0)
    .into()
}

fn update_channel(index: i32) -> UpdateChannel {
    match index {
        1 => UpdateChannel::Preview,
        _ => UpdateChannel::Stable,
    }
}

fn tun_policy(index: i32) -> TunLaunchPolicy {
    match index {
        1 => TunLaunchPolicy::DenyUntilServiceInstalled,
        _ => TunLaunchPolicy::RequireService,
    }
}
