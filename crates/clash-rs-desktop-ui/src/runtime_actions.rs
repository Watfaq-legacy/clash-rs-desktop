use std::sync::{Mutex, OnceLock};

use clash_rs_desktop_core::{
    ActiveCore, CoreRuntimeManager, CoreState, ProfileStore, ServiceCommand,
    runtime_options_from_active_profile,
};

use crate::{
    paths::{app_pipe_path, global_override_path, reactor_paths},
    service_actions::{describe_service_response, send_service_command_sync},
};

pub(crate) fn managed_runtime() -> &'static Mutex<Option<CoreRuntimeManager>> {
    static MANAGED_RUNTIME: OnceLock<Mutex<Option<CoreRuntimeManager>>> = OnceLock::new();
    MANAGED_RUNTIME.get_or_init(|| Mutex::new(None))
}

pub(crate) fn reactor_start_core_text() -> String {
    let mut runtime = match managed_runtime().lock() {
        Ok(runtime) => runtime,
        Err(error) => return format!("start failed: runtime state is poisoned ({error})"),
    };

    if let Some(manager) = runtime.as_mut() {
        let state = manager.state();
        if matches!(
            state,
            CoreState::Starting { .. } | CoreState::Running { .. }
        ) {
            return format!("start: core is already {}", describe_core_state(&state));
        }
    }

    let mut manager = match reactor_runtime_manager() {
        Ok(manager) => manager,
        Err(error) => return format!("start failed: {error}"),
    };

    match manager.start_validated() {
        Ok(CoreState::NeedsService { reasons }) => {
            let request = manager.launch_request();
            let reason_text = describe_elevation_reasons(&reasons);
            *runtime = None;
            drop(runtime);
            match send_service_command_sync(ServiceCommand::StartCore { request }) {
                Ok(response) => format!(
                    "start used the Windows service\nreasons: {reason_text}\n{}",
                    describe_service_response(&response)
                ),
                Err(error) => format!(
                    "start needs the Windows service\nreasons: {reason_text}\nservice request failed: {error}\nstart the service pipe host with: cargo run -p clash-rs-desktop-service -- --pipe"
                ),
            }
        }
        Ok(state) => {
            let message = format!("start: {}", describe_core_state(&state));
            *runtime = Some(manager);
            message
        }
        Err(error) => {
            *runtime = None;
            format!("start failed: {error}")
        }
    }
}

pub(crate) fn reactor_stop_core_text() -> String {
    let mut runtime = match managed_runtime().lock() {
        Ok(runtime) => runtime,
        Err(error) => return format!("stop failed: runtime state is poisoned ({error})"),
    };

    let Some(manager) = runtime.as_mut() else {
        drop(runtime);
        return match send_service_command_sync(ServiceCommand::StopCore) {
            Ok(response) => format!(
                "stop: no direct GUI-owned core was running\n{}",
                describe_service_response(&response)
            ),
            Err(error) => {
                format!("stop: no direct GUI-owned core was running\nservice stop failed: {error}")
            }
        };
    };

    match manager.stop() {
        Ok(state) => {
            *runtime = None;
            format!("stop: {}", describe_core_state(&state))
        }
        Err(error) => format!("stop failed: {error}"),
    }
}

pub(crate) fn reactor_restart_core_text() -> String {
    let stop = reactor_stop_core_text();
    let start = reactor_start_core_text();
    format!("restart\n{stop}\n{start}")
}

pub(crate) fn reactor_runtime_manager() -> anyhow::Result<CoreRuntimeManager> {
    let paths = reactor_paths()?;
    let active_core = ActiveCore::load(paths.active_core_metadata_path())?;
    let mut profile_store = ProfileStore::load(paths.profile_manifest_path())?;
    profile_store.ensure_default_profile(&paths)?;
    profile_store.save()?;

    let options = runtime_options_from_active_profile(
        paths.clone(),
        active_core,
        &profile_store,
        Some(global_override_path(&paths)),
        app_pipe_path(),
    )?;

    Ok(CoreRuntimeManager::new(options))
}

pub(crate) fn describe_core_state(state: &CoreState) -> String {
    match state {
        CoreState::Stopped => "stopped".to_owned(),
        CoreState::Starting { elevated } => format!("starting (elevated: {elevated})"),
        CoreState::Running { elevated, owner } => {
            format!("running (elevated: {elevated}, owner: {owner:?})")
        }
        CoreState::NeedsService { reasons } => format!(
            "needs Windows service ({})",
            describe_elevation_reasons(reasons)
        ),
        CoreState::Stopping => "stopping".to_owned(),
        CoreState::Failed { message } => format!("failed ({message})"),
    }
}

pub(crate) fn describe_elevation_reasons(
    reasons: &[clash_rs_desktop_core::ElevationReason],
) -> String {
    if reasons.is_empty() {
        return "none".to_owned();
    }

    reasons
        .iter()
        .map(|reason| format!("{reason:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}
