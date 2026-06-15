use clash_rs_desktop_core::{
    ActiveCore, CoreState, CoreUpdateManager, CoreUpdateSettings, ManagedCoreFiles, ServiceCommand,
    ServiceResponse, SettingsStore, UpdateDecision, pipe_transport,
};

use crate::paths::reactor_paths;
use crate::runtime_actions::{reactor_start_core_text, reactor_stop_core_text};

/// One-line summary of the currently installed core, or a clear "not installed"
/// message. Backs the status line in Settings → Updates.
pub(crate) fn reactor_core_status_text() -> String {
    match load_active_core() {
        Some(core) => format!(
            "Installed: v{} ({}, {} channel)",
            core.version,
            core.release_tag,
            channel_label(&core),
        ),
        None => "No core installed yet. Use \"Download / install core\" to fetch one.".to_owned(),
    }
}

/// Check GitHub for the latest release on the configured channel and report
/// whether an update (or first install) is available. Read-only — touches no
/// files and never stops the core.
pub(crate) fn reactor_check_core_update_text() -> String {
    let settings = match load_update_settings() {
        Ok(settings) => settings,
        Err(error) => return format!("check for updates failed: {error}"),
    };
    let active = load_active_core();

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return format!("check for updates failed: {error}"),
    };

    let manager = CoreUpdateManager::new();
    match runtime.block_on(manager.check_latest(&settings, active.as_ref())) {
        Ok(result) => describe_update_decision(&result.decision),
        Err(error) => format!("check for updates failed: {error}"),
    }
}

/// Download the latest core for the configured channel and activate it. Handles
/// both first-run install and live upgrade: because `clash-rs.exe` is locked
/// while the core runs, a running core is stopped before the swap and restarted
/// after, so the user ends up back in their prior state on the new version.
pub(crate) fn reactor_download_core_text() -> String {
    let paths = match reactor_paths() {
        Ok(paths) => paths,
        Err(error) => return format!("download core failed: {error}"),
    };
    let settings = match load_update_settings() {
        Ok(settings) => settings,
        Err(error) => return format!("download core failed: {error}"),
    };
    let active = load_active_core();

    // Stop a running core first — the binary can't be overwritten while open.
    let was_running = core_is_running();
    if was_running {
        let stop = reactor_stop_core_text();
        if crate::views::dashboard::looks_like_failure(&stop) {
            return format!(
                "download core aborted: could not stop the running core before updating\n{stop}"
            );
        }
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return format!("download core failed: {error}"),
    };

    let manager = CoreUpdateManager::new();
    let files = ManagedCoreFiles::from_paths(&paths);

    let outcome = runtime.block_on(async {
        let check = manager.check_latest(&settings, active.as_ref()).await?;
        let activated = manager
            .download_and_activate(&check.candidate, &files)
            .await?;
        anyhow::Ok(activated)
    });

    let activated = match outcome {
        Ok(activated) => activated,
        Err(error) => {
            // The binary was not swapped; bring the core back if we stopped it.
            let restart_note = if was_running {
                format!("\nrestarting previous core...\n{}", reactor_start_core_text())
            } else {
                String::new()
            };
            return format!("download core failed: {error}{restart_note}");
        }
    };

    let mut report = format!(
        "core installed: v{} ({})",
        activated.version, activated.asset_name
    );
    if was_running {
        let start = reactor_start_core_text();
        report.push_str(&format!("\nrestarted core on the new version\n{start}"));
    }
    report
}

/// Human-facing message for an update decision. Pure (no I/O) so the wording is
/// unit-testable without a network round-trip.
pub(crate) fn describe_update_decision(decision: &UpdateDecision) -> String {
    match decision {
        UpdateDecision::UpToDate => "Core is up to date.".to_owned(),
        UpdateDecision::UpdateAvailable { current, latest } => {
            format!("Update available: {current} → {latest}. Use \"Download / install core\".")
        }
        UpdateDecision::NotInstalled { latest } => {
            format!("No core installed. Latest is {latest} — use \"Download / install core\".")
        }
    }
}

fn channel_label(core: &ActiveCore) -> &'static str {
    match core.channel {
        clash_rs_desktop_core::UpdateChannel::Stable => "stable",
        clash_rs_desktop_core::UpdateChannel::Preview => "preview",
    }
}

fn load_active_core() -> Option<ActiveCore> {
    let paths = reactor_paths().ok()?;
    ActiveCore::load(paths.active_core_metadata_path()).ok()
}

fn load_update_settings() -> anyhow::Result<CoreUpdateSettings> {
    let paths = reactor_paths()?;
    let store = SettingsStore::load(paths.settings_path())?;
    Ok(store.settings().updates.clone())
}

fn core_is_running() -> bool {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return false,
    };

    runtime.block_on(async {
        let client = pipe_transport::ServicePipeClient::new(
            clash_rs_desktop_core::service::SERVICE_PIPE,
        );
        matches!(
            client.send(&ServiceCommand::Status).await,
            Ok(ServiceResponse::State {
                state: CoreState::Running { .. }
            })
        )
    })
}

#[cfg(test)]
mod tests {
    use super::describe_update_decision;
    use clash_rs_desktop_core::UpdateDecision;

    #[test]
    fn decision_messages_are_actionable() {
        assert_eq!(
            describe_update_decision(&UpdateDecision::UpToDate),
            "Core is up to date."
        );
        assert!(
            describe_update_decision(&UpdateDecision::UpdateAvailable {
                current: "v0.10.5".to_owned(),
                latest: "v0.10.6".to_owned(),
            })
            .contains("v0.10.5 → v0.10.6")
        );
        assert!(
            describe_update_decision(&UpdateDecision::NotInstalled {
                latest: "v0.10.6".to_owned(),
            })
            .contains("No core installed")
        );
    }
}
