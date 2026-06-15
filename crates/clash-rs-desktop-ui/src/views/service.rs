use windows_reactor::*;

use crate::service_actions::{
    reactor_service_install_text, reactor_service_process_stop_text, reactor_service_start_text,
    reactor_service_status_text, reactor_service_stop_text, reactor_service_uninstall_text,
};
use crate::views::dashboard::report_action;
use crate::views::design::{ToastQueue, section, selectable_text, spacing, toolbar, type_scale};
use crate::views::output::{SafetyState, confirm_label, guarded_action};

#[derive(Clone)]
pub(crate) struct ServiceState {
    pub(crate) status: String,
    pub(crate) set_status: AsyncSetState<String>,
}

fn spawn_service_action<F>(set_status: AsyncSetState<String>, toasts: ToastQueue, label: &'static str, op: F)
where
    F: FnOnce() -> String + Send + 'static,
{
    std::thread::spawn(move || {
        let result = op();
        report_action(&toasts, label, &result);
        set_status.call(reactor_service_status_text());
    });
}

pub(crate) fn service_toolbar(
    state: &ServiceState,
    safety: &SafetyState,
    toasts: &ToastQueue,
) -> Element {
    vstack((
        section(
            "Service state",
            Some("Live status reported by the Windows service controller."),
            vstack((
                status_box(&state.status),
                toolbar(hstack((
                    button("Check status")
                        .icon(SymbolGlyph::Find)
                        .on_click({
                            let set_status = state.set_status.clone();
                            move || {
                                let set_status = set_status.clone();
                                std::thread::spawn(move || {
                                    set_status.call(reactor_service_status_text());
                                });
                            }
                        })
                        .accent(),
                    button("Start")
                        .icon(SymbolGlyph::Play)
                        .on_click({
                            let set_status = state.set_status.clone();
                            let toasts = toasts.clone();
                            move || {
                                spawn_service_action(
                                    set_status.clone(),
                                    toasts.clone(),
                                    "Service start requested",
                                    reactor_service_start_text,
                                );
                            }
                        })
                        .subtle(),
                    button(confirm_label(
                        safety,
                        "stop-service-process",
                        "Stop",
                        "Confirm stop",
                    ))
                    .icon(SymbolGlyph::Pause)
                    .on_click({
                        let set_status = state.set_status.clone();
                        let safety = safety.clone();
                        let toasts = toasts.clone();
                        move || {
                            let set_status = set_status.clone();
                            let toasts = toasts.clone();
                            guarded_action(&safety, "stop-service-process", move || {
                                spawn_service_action(
                                    set_status,
                                    toasts,
                                    "Stop service process",
                                    reactor_service_process_stop_text,
                                );
                            });
                        }
                    })
                    .subtle(),
                ))
                .spacing(spacing::SM)),
            ))
            .spacing(spacing::SM + 2.0),
        ),
        section(
            "Install and maintenance",
            Some(
                "The Windows service runs as SYSTEM so TUN mode and elevated cores start without UAC prompts.",
            ),
            toolbar(hstack((
                button("Install")
                    .icon(SymbolGlyph::Download)
                    .on_click({
                        let set_status = state.set_status.clone();
                        let toasts = toasts.clone();
                        move || {
                            spawn_service_action(
                                set_status.clone(),
                                toasts.clone(),
                                "Service installation requested",
                                reactor_service_install_text,
                            );
                        }
                    })
                    .subtle(),
                button(confirm_label(
                    safety,
                    "uninstall-service",
                    "Uninstall",
                    "Confirm uninstall",
                ))
                .icon(SymbolGlyph::Delete)
                .on_click({
                    let safety = safety.clone();
                    let set_status = state.set_status.clone();
                    let toasts = toasts.clone();
                    move || {
                        let set_status = set_status.clone();
                        let toasts = toasts.clone();
                        guarded_action(&safety, "uninstall-service", move || {
                            spawn_service_action(
                                set_status,
                                toasts,
                                "Uninstall service",
                                reactor_service_uninstall_text,
                            );
                        });
                    }
                })
                .subtle(),
                button(confirm_label(
                    safety,
                    "stop-service-core",
                    "Stop core",
                    "Confirm stop core",
                ))
                .icon(SymbolGlyph::Cancel)
                .on_click({
                    let safety = safety.clone();
                    let set_status = state.set_status.clone();
                    let toasts = toasts.clone();
                    move || {
                        let set_status = set_status.clone();
                        let toasts = toasts.clone();
                        guarded_action(&safety, "stop-service-core", move || {
                            spawn_service_action(
                                set_status,
                                toasts,
                                "Stop core via service",
                                reactor_service_stop_text,
                            );
                        });
                    }
                })
                .subtle(),
            ))
            .spacing(spacing::SM)),
        ),
    ))
    .spacing(spacing::MD + 2.0)
    .into()
}

fn status_box(status: &str) -> Element {
    border(scroll_view(selectable_text(status, type_scale::CAPTION)).height(96.0))
        .padding(Thickness::uniform(spacing::SM + 2.0))
        .corner_radius(8.0)
        .background(tokens::ControlFill)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .into()
}
