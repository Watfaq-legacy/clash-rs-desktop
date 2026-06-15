use clash_rs_desktop_core::{
    ActiveCore, CoreRuntimeManager, ProfileStore, ServiceCommand,
    runtime_options_from_active_profile,
};

use crate::{
    paths::{app_pipe_path, global_override_path, reactor_paths},
    service_actions::{describe_service_response, send_service_command_sync},
};

pub(crate) fn reactor_start_core_text() -> String {
    let mut manager = match reactor_runtime_manager() {
        Ok(manager) => manager,
        Err(error) => return format!("start failed: {error}"),
    };

    if let Err(error) = manager.generate_config() {
        return format!("start failed: config generation error: {error}");
    }

    if let Err(error) = manager.validate_config() {
        return format!("start failed: config validation error: {error}");
    }

    let request = manager.launch_request();
    match send_service_command_sync(ServiceCommand::StartCore { request }) {
        Ok(response) => format!("start: {}", describe_service_response(&response)),
        Err(error) => format!(
            "start failed: service unreachable: {error}\nstart the service with: cargo run -p clash-rs-desktop-service -- --pipe"
        ),
    }
}

pub(crate) fn reactor_stop_core_text() -> String {
    match send_service_command_sync(ServiceCommand::StopCore) {
        Ok(response) => format!("stop: {}", describe_service_response(&response)),
        Err(error) => format!("stop failed: service unreachable: {error}"),
    }
}

pub(crate) fn reactor_restart_core_text() -> String {
    let mut manager = match reactor_runtime_manager() {
        Ok(manager) => manager,
        Err(error) => return format!("restart failed: {error}"),
    };

    if let Err(error) = manager.generate_config() {
        return format!("restart failed: config generation error: {error}");
    }

    if let Err(error) = manager.validate_config() {
        return format!("restart failed: config validation error: {error}");
    }

    let request = manager.launch_request();
    match send_service_command_sync(ServiceCommand::RestartCore { request }) {
        Ok(response) => format!("restart: {}", describe_service_response(&response)),
        Err(error) => format!("restart failed: service unreachable: {error}"),
    }
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
