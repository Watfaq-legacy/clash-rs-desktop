use windows_reactor::*;

use crate::profile_actions::{
    reactor_delete_profile_text, reactor_download_profile_text,
    reactor_ensure_default_profile_text, reactor_import_profile_text,
    reactor_load_profile_override_text, reactor_profile_details_text,
    reactor_profile_override_form, reactor_save_profile_override_form_text,
    reactor_save_profile_override_text, reactor_set_active_profile_text,
    reactor_update_profile_source_text,
};
use crate::settings_actions::{reactor_load_global_override_text, reactor_save_global_override_text};
use crate::views::dashboard::{looks_like_failure, report_action, trim_one_line};
use crate::views::design::{Tone, ToastQueue, empty_state, section, spacing, type_scale};
use crate::views::output::{SafetyState, confirm_label, guarded_action};

#[derive(Clone)]
pub(crate) struct ProfilesState {
    pub(crate) profile_index: i32,
    pub(crate) set_profile_index: SetState<i32>,
    pub(crate) selected_profile_index: i32,
    pub(crate) profile_items: Vec<String>,
    pub(crate) import_name: String,
    pub(crate) set_import_name: SetState<String>,
    pub(crate) import_path: String,
    pub(crate) set_import_path: SetState<String>,
    pub(crate) remote_name: String,
    pub(crate) set_remote_name: SetState<String>,
    pub(crate) remote_url: String,
    pub(crate) set_remote_url: SetState<String>,
    pub(crate) profile_override: String,
    pub(crate) set_profile_override: AsyncSetState<String>,
    pub(crate) override_mode: String,
    pub(crate) set_override_mode: AsyncSetState<String>,
    pub(crate) override_log_level: String,
    pub(crate) set_override_log_level: AsyncSetState<String>,
    pub(crate) override_mixed_port: String,
    pub(crate) set_override_mixed_port: AsyncSetState<String>,
    pub(crate) override_allow_lan: bool,
    pub(crate) set_override_allow_lan: AsyncSetState<bool>,
    pub(crate) override_ipv6: bool,
    pub(crate) set_override_ipv6: AsyncSetState<bool>,
    pub(crate) profiles_version: u32,
    pub(crate) set_profiles_version: AsyncSetState<u32>,
    pub(crate) global_override: String,
    pub(crate) set_global_override: AsyncSetState<String>,
}

pub(crate) fn profiles_section(
    state: &ProfilesState,
    safety: &SafetyState,
    toasts: &ToastQueue,
) -> Element {
    vstack((
        section(
            "Profiles",
            Some("Activate a profile, import a YAML file, or pull a subscription URL."),
            profile_workspace(state, safety, toasts),
        ),
        section(
            "Profile override",
            Some("Per-profile knobs applied at activation time. Advanced users can edit raw YAML."),
            profile_override_section(state, toasts),
        ),
        section(
            "Global runtime override",
            Some("YAML applied on top of every profile when the core starts."),
            global_override_editor(state, toasts),
        ),
    ))
    .spacing(spacing::MD + 2.0)
    .into()
}

fn profile_workspace(state: &ProfilesState, safety: &SafetyState, toasts: &ToastQueue) -> Element {
    hstack((
        profile_list(state, safety, toasts).width(360.0),
        vstack((
            profile_import(state, toasts),
            profile_download(state, toasts),
        ))
        .spacing(spacing::MD),
    ))
    .spacing(spacing::MD + 4.0)
    .into()
}

fn profile_list(state: &ProfilesState, safety: &SafetyState, toasts: &ToastQueue) -> Element {
    let has_profiles = state
        .profile_items
        .first()
        .map(|first| first != "No profiles configured")
        .unwrap_or(false);
    if !has_profiles {
        return empty_state(
            "No profiles yet",
            "Import a YAML file or pull a subscription URL to get started.",
            Some(
                button("Ensure Default")
                    .icon(SymbolGlyph::Add)
                    .on_click({
                        let set_profiles_version = state.set_profiles_version.clone();
                        let profiles_version = state.profiles_version;
                        let toasts = toasts.clone();
                        move || {
                            let set_profiles_version = set_profiles_version.clone();
                            let toasts = toasts.clone();
                            std::thread::spawn(move || {
                                let result = reactor_ensure_default_profile_text();
                                set_profiles_version.call(profiles_version.wrapping_add(1));
                                report_action(&toasts, "Default profile created", &result);
                            });
                        }
                    })
                    .accent()
                    .into(),
            ),
        );
    }
    vstack((
        list_box()
            .items(state.profile_items.clone())
            .selected_index(state.selected_profile_index)
            .on_selection_changed({
                let set_profile_index = state.set_profile_index.clone();
                move |index| {
                    set_profile_index.call(index);
                    std::thread::spawn(move || {
                        reactor_profile_details_text(index);
                    });
                }
            }),
        hstack((
            button("Activate")
                .icon(SymbolGlyph::Accept)
                .on_click({
                    let profile_index = state.profile_index;
                    let set_profiles_version = state.set_profiles_version.clone();
                    let profiles_version = state.profiles_version;
                    let toasts = toasts.clone();
                    move || {
                        let set_profiles_version = set_profiles_version.clone();
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let result = reactor_set_active_profile_text(profile_index);
                            set_profiles_version.call(profiles_version.wrapping_add(1));
                            report_action(&toasts, "Profile activated", &result);
                        });
                    }
                })
                .accent(),
            button("Update Source")
                .icon(SymbolGlyph::Sync)
                .on_click({
                    let profile_index = state.profile_index;
                    let toasts = toasts.clone();
                    move || {
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let result = reactor_update_profile_source_text(profile_index);
                            report_action(&toasts, "Profile source refreshed", &result);
                        });
                    }
                })
                .subtle(),
            button(confirm_label(
                safety,
                "delete-profile",
                "Delete",
                "Confirm Delete",
            ))
            .icon(SymbolGlyph::Delete)
            .on_click({
                let safety = safety.clone();
                let profile_index = state.profile_index;
                let set_profiles_version = state.set_profiles_version.clone();
                let profiles_version = state.profiles_version;
                let toasts = toasts.clone();
                move || {
                    let set_profiles_version = set_profiles_version.clone();
                    let toasts = toasts.clone();
                    guarded_action(&safety, "delete-profile", move || {
                        std::thread::spawn(move || {
                            let result = reactor_delete_profile_text(profile_index);
                            set_profiles_version.call(profiles_version.wrapping_add(1));
                            if looks_like_failure(&result) {
                                toasts.push(
                                    "Delete profile failed",
                                    trim_one_line(&result),
                                    Tone::Critical,
                                );
                            }
                        });
                    })
                }
            })
            .subtle(),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::SM + 2.0)
    .into()
}

fn profile_import(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    vstack((
        text_block("Import YAML")
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        text_box(&state.import_name)
            .header("Name")
            .placeholder("Home, Work, Travel")
            .on_changed({
                let set_import_name = state.set_import_name.clone();
                move |value| set_import_name.call(value)
            })
            .width(280.0),
        text_box(&state.import_path)
            .header("YAML file path")
            .placeholder(r"C:\Users\you\Downloads\config.yaml")
            .on_changed({
                let set_import_path = state.set_import_path.clone();
                move |value| set_import_path.call(value)
            })
            .width(440.0),
        button("Import")
            .icon(SymbolGlyph::Download)
            .on_click({
                let import_name = state.import_name.clone();
                let import_path = state.import_path.clone();
                let set_profiles_version = state.set_profiles_version.clone();
                let profiles_version = state.profiles_version;
                let toasts = toasts.clone();
                move || {
                    let import_name = import_name.clone();
                    let import_path = import_path.clone();
                    let set_profiles_version = set_profiles_version.clone();
                    let toasts = toasts.clone();
                    std::thread::spawn(move || {
                        let result = reactor_import_profile_text(&import_name, &import_path);
                        set_profiles_version.call(profiles_version.wrapping_add(1));
                        report_action(&toasts, "Profile imported", &result);
                    });
                }
            })
            .subtle(),
    ))
    .spacing(spacing::SM)
    .into()
}

fn profile_download(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    vstack((
        text_block("Subscription URL")
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        text_box(&state.remote_name)
            .header("Name")
            .placeholder("Airport subscription")
            .on_changed({
                let set_remote_name = state.set_remote_name.clone();
                move |value| set_remote_name.call(value)
            })
            .width(280.0),
        text_box(&state.remote_url)
            .header("URL")
            .placeholder("https://example.com/sub.yaml")
            .on_changed({
                let set_remote_url = state.set_remote_url.clone();
                move |value| set_remote_url.call(value)
            })
            .width(440.0),
        button("Download")
            .icon(SymbolGlyph::Download)
            .on_click({
                let remote_name = state.remote_name.clone();
                let remote_url = state.remote_url.clone();
                let set_profiles_version = state.set_profiles_version.clone();
                let profiles_version = state.profiles_version;
                let toasts = toasts.clone();
                move || {
                    let remote_name = remote_name.clone();
                    let remote_url = remote_url.clone();
                    let set_profiles_version = set_profiles_version.clone();
                    let toasts = toasts.clone();
                    std::thread::spawn(move || {
                        let result = reactor_download_profile_text(&remote_name, &remote_url);
                        set_profiles_version.call(profiles_version.wrapping_add(1));
                        report_action(&toasts, "Subscription downloaded", &result);
                    });
                }
            })
            .subtle(),
    ))
    .spacing(spacing::SM)
    .into()
}

fn profile_override_section(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    hstack((
        profile_override_builder(state, toasts).width(440.0),
        profile_override_editor(state, toasts),
    ))
    .spacing(spacing::MD + 4.0)
    .into()
}

fn profile_override_builder(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    vstack((
        text_block("Common knobs")
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        hstack((
            ComboBox::new(["", "rule", "global", "direct"])
                .header("Mode")
                .selected_index(choice_index(
                    &state.override_mode,
                    &["", "rule", "global", "direct"],
                ))
                .on_selection_changed({
                    let set_override_mode = state.set_override_mode.clone();
                    move |index| {
                        set_choice(index, &["", "rule", "global", "direct"], &set_override_mode)
                    }
                })
                .width(180.0),
            ComboBox::new(["", "info", "warning", "error", "debug", "trace", "silent"])
                .header("Log level")
                .selected_index(choice_index(
                    &state.override_log_level,
                    &["", "info", "warning", "error", "debug", "trace", "silent"],
                ))
                .on_selection_changed({
                    let set_override_log_level = state.set_override_log_level.clone();
                    move |index| {
                        set_choice(
                            index,
                            &["", "info", "warning", "error", "debug", "trace", "silent"],
                            &set_override_log_level,
                        )
                    }
                })
                .width(180.0),
        ))
        .spacing(spacing::SM),
        text_box(&state.override_mixed_port)
            .header("Mixed port")
            .placeholder("7890")
            .on_changed({
                let set_override_mixed_port = state.set_override_mixed_port.clone();
                move |value| set_override_mixed_port.call(value)
            })
            .width(140.0),
        hstack((
            ToggleSwitch::new(state.override_allow_lan)
                .header("Allow LAN")
                .on_content("On")
                .off_content("Off")
                .on_changed({
                    let set_override_allow_lan = state.set_override_allow_lan.clone();
                    move |value| set_override_allow_lan.call(value)
                }),
            ToggleSwitch::new(state.override_ipv6)
                .header("IPv6")
                .on_content("On")
                .off_content("Off")
                .on_changed({
                    let set_override_ipv6 = state.set_override_ipv6.clone();
                    move |value| set_override_ipv6.call(value)
                }),
        ))
        .spacing(spacing::MD),
        button("Save Override")
            .icon(SymbolGlyph::Save)
            .on_click({
                let profile_index = state.profile_index;
                let mode = state.override_mode.clone();
                let log_level = state.override_log_level.clone();
                let mixed_port = state.override_mixed_port.clone();
                let allow_lan = state.override_allow_lan;
                let ipv6 = state.override_ipv6;
                let toasts = toasts.clone();
                move || {
                    let mode = mode.clone();
                    let log_level = log_level.clone();
                    let mixed_port = mixed_port.clone();
                    let toasts = toasts.clone();
                    std::thread::spawn(move || {
                        let result = reactor_save_profile_override_form_text(
                            profile_index,
                            &mode,
                            &log_level,
                            &mixed_port,
                            allow_lan,
                            ipv6,
                        );
                        report_action(&toasts, "Profile override saved", &result);
                    });
                }
            })
            .accent(),
    ))
    .spacing(spacing::SM + 2.0)
    .into()
}

fn profile_override_editor(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    vstack((
        text_block("Advanced YAML")
            .font_size(type_scale::CAPTION)
            .foreground(tokens::SecondaryText),
        text_box(&state.profile_override)
            .multiline()
            .header("Free-form YAML merged on top of the profile")
            .placeholder("mode: rule")
            .on_changed({
                let set_profile_override = state.set_profile_override.clone();
                move |value| set_profile_override.call(value)
            }),
        hstack((
            button("Load")
                .icon(SymbolGlyph::Download)
                .on_click({
                    let set_profile_override = state.set_profile_override.clone();
                    let set_override_mode = state.set_override_mode.clone();
                    let set_override_log_level = state.set_override_log_level.clone();
                    let set_override_mixed_port = state.set_override_mixed_port.clone();
                    let set_override_allow_lan = state.set_override_allow_lan.clone();
                    let set_override_ipv6 = state.set_override_ipv6.clone();
                    let profile_index = state.profile_index;
                    move || {
                        let set_profile_override = set_profile_override.clone();
                        let set_override_mode = set_override_mode.clone();
                        let set_override_log_level = set_override_log_level.clone();
                        let set_override_mixed_port = set_override_mixed_port.clone();
                        let set_override_allow_lan = set_override_allow_lan.clone();
                        let set_override_ipv6 = set_override_ipv6.clone();
                        std::thread::spawn(move || {
                            let contents = reactor_load_profile_override_text(profile_index);
                            let form = reactor_profile_override_form(profile_index);
                            set_profile_override.call(contents);
                            set_override_mode.call(form.mode);
                            set_override_log_level.call(form.log_level);
                            set_override_mixed_port.call(form.mixed_port);
                            set_override_allow_lan.call(form.allow_lan);
                            set_override_ipv6.call(form.ipv6);
                        });
                    }
                })
                .subtle(),
            button("Save")
                .icon(SymbolGlyph::Save)
                .on_click({
                    let profile_index = state.profile_index;
                    let profile_override = state.profile_override.clone();
                    let toasts = toasts.clone();
                    move || {
                        let profile_override = profile_override.clone();
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let result = reactor_save_profile_override_text(
                                profile_index,
                                &profile_override,
                            );
                            report_action(&toasts, "Profile YAML override saved", &result);
                        });
                    }
                })
                .accent(),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::SM)
    .into()
}

fn global_override_editor(state: &ProfilesState, toasts: &ToastQueue) -> Element {
    vstack((
        text_box(&state.global_override)
            .multiline()
            .header("YAML")
            .placeholder("log-level: info")
            .on_changed({
                let set_global_override = state.set_global_override.clone();
                move |value| set_global_override.call(value)
            }),
        hstack((
            button("Load")
                .icon(SymbolGlyph::Download)
                .on_click({
                    let set_global_override = state.set_global_override.clone();
                    let toasts = toasts.clone();
                    move || {
                        let set_global_override = set_global_override.clone();
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let contents = reactor_load_global_override_text();
                            let snapshot = contents.clone();
                            set_global_override.call(contents);
                            report_action(&toasts, "Loaded global override", &snapshot);
                        });
                    }
                })
                .subtle(),
            button("Save")
                .icon(SymbolGlyph::Save)
                .on_click({
                    let global_override = state.global_override.clone();
                    let toasts = toasts.clone();
                    move || {
                        let global_override = global_override.clone();
                        let toasts = toasts.clone();
                        std::thread::spawn(move || {
                            let result = reactor_save_global_override_text(&global_override);
                            report_action(&toasts, "Global override saved", &result);
                        });
                    }
                })
                .accent(),
        ))
        .spacing(spacing::SM),
    ))
    .spacing(spacing::SM)
    .into()
}

fn choice_index(value: &str, choices: &[&str]) -> i32 {
    choices
        .iter()
        .position(|choice| choice.eq_ignore_ascii_case(value.trim()))
        .map(|index| index as i32)
        .unwrap_or(0)
}

fn set_choice(index: i32, choices: &[&str], setter: &AsyncSetState<String>) {
    if let Some(choice) = choices.get(index.max(0) as usize) {
        setter.call((*choice).to_owned());
    }
}
