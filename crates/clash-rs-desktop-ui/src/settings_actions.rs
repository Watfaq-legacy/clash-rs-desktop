
use clash_rs_desktop_core::{
    AppPaths, SettingsStore, SystemProxyBackend, SystemProxySnapshot, SystemProxyTarget,
    TunLaunchPolicy, UpdateChannel, WindowsArch, WindowsSystemProxy,
};

use crate::controller_endpoint::{
    ControllerEndpointInputs, controller_inputs, save_controller_endpoint_text,
};
use crate::paths::{global_override_path, reactor_paths};

#[derive(Clone, Debug)]
pub(crate) struct SettingsInputs {
    pub(crate) update_channel: UpdateChannel,
    pub(crate) auto_check: bool,
    pub(crate) tun_policy: TunLaunchPolicy,
    pub(crate) proxy_host: String,
    pub(crate) proxy_port: String,
    pub(crate) proxy_bypass: String,
}

pub(crate) fn reactor_settings_inputs() -> SettingsInputs {
    let settings = reactor_paths()
        .and_then(|paths| SettingsStore::load(paths.settings_path()))
        .map(|store| store.settings().clone())
        .unwrap_or_default();
    let target = settings.system_proxy.target;

    let _ = update_settings(|settings| settings.updates.architecture = WindowsArch::detect());

    SettingsInputs {
        update_channel: settings.updates.channel,
        auto_check: settings.updates.auto_check,
        tun_policy: settings.tun,
        proxy_host: target.host.clone(),
        proxy_port: target.port.to_string(),
        proxy_bypass: target.bypass_list(),
    }
}

pub(crate) fn reactor_controller_inputs() -> ControllerEndpointInputs {
    controller_inputs()
}

pub(crate) fn reactor_save_controller_endpoint_text(endpoint: &str, secret: &str) -> String {
    save_controller_endpoint_text(endpoint, secret)
}

pub(crate) fn reactor_set_update_channel_text(channel: UpdateChannel) -> String {
    match update_settings(|settings| settings.updates.channel = channel) {
        Ok(()) => format!(
            "settings: update channel set to {channel:?}\n{}",
            settings_text()
        ),
        Err(error) => format!("settings: failed to set update channel ({error})"),
    }
}

pub(crate) fn reactor_toggle_auto_check_text() -> String {
    match update_settings(|settings| settings.updates.auto_check = !settings.updates.auto_check) {
        Ok(()) => settings_text(),
        Err(error) => format!("settings: failed to toggle auto check ({error})"),
    }
}

pub(crate) fn reactor_set_tun_policy_text(policy: TunLaunchPolicy) -> String {
    match update_settings(|settings| settings.tun = policy) {
        Ok(()) => format!(
            "settings: TUN policy set to {policy:?}\n{}",
            settings_text()
        ),
        Err(error) => format!("settings: failed to set TUN policy ({error})"),
    }
}

pub(crate) fn reactor_save_proxy_target_text(host: &str, port: &str, bypass: &str) -> String {
    let target = match parse_proxy_target(host, port, bypass) {
        Ok(target) => target,
        Err(error) => return format!("settings: invalid proxy target ({error})"),
    };

    match update_settings(|settings| settings.system_proxy.target = target.clone()) {
        Ok(()) => format!(
            "settings: proxy target saved as {}\n{}",
            target.server(),
            settings_text()
        ),
        Err(error) => format!("settings: failed to save proxy target ({error})"),
    }
}

pub(crate) fn reactor_apply_system_proxy_text() -> String {
    match apply_system_proxy() {
        Ok(snapshot) => format!(
            "system proxy applied\nprevious enabled: {}\nprevious server: {}\n{}",
            snapshot.enabled,
            snapshot.server.unwrap_or_else(|| "none".to_owned()),
            settings_text()
        ),
        Err(error) => format!("system proxy apply failed: {error}"),
    }
}

pub(crate) fn reactor_restore_system_proxy_text() -> String {
    match restore_system_proxy() {
        Ok(()) => format!("system proxy restored\n{}", settings_text()),
        Err(error) => format!("system proxy restore failed: {error}"),
    }
}

fn parse_proxy_target(host: &str, port: &str, bypass: &str) -> anyhow::Result<SystemProxyTarget> {
    let host = host.trim();
    if host.is_empty() {
        anyhow::bail!("host is required");
    }

    let port = port.trim().parse::<u16>()?;
    let bypass = bypass
        .split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    Ok(SystemProxyTarget {
        host: host.to_owned(),
        port,
        bypass,
    })
}

fn update_settings(
    update: impl FnOnce(&mut clash_rs_desktop_core::AppSettings),
) -> anyhow::Result<()> {
    let paths = reactor_paths()?;
    let mut store = SettingsStore::load(paths.settings_path())?;
    update(store.settings_mut());
    store.save()
}

fn apply_system_proxy() -> anyhow::Result<SystemProxySnapshot> {
    let paths = reactor_paths()?;
    let mut store = SettingsStore::load(paths.settings_path())?;
    let backend = WindowsSystemProxy;
    let snapshot = backend.snapshot()?;
    backend.apply(&store.settings().system_proxy.target)?;
    save_proxy_snapshot(&paths, &snapshot)?;
    store.settings_mut().system_proxy.enabled = true;
    store.save()?;
    Ok(snapshot)
}

fn restore_system_proxy() -> anyhow::Result<()> {
    let paths = reactor_paths()?;
    let mut store = SettingsStore::load(paths.settings_path())?;
    let snapshot = load_proxy_snapshot(&paths)?;
    WindowsSystemProxy.restore(&snapshot)?;
    store.settings_mut().system_proxy.enabled = false;
    store.save()?;
    Ok(())
}

fn save_proxy_snapshot(paths: &AppPaths, snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.program_data)?;
    std::fs::write(
        proxy_snapshot_path(paths),
        serde_json::to_string_pretty(snapshot)?,
    )?;
    Ok(())
}

fn load_proxy_snapshot(paths: &AppPaths) -> anyhow::Result<SystemProxySnapshot> {
    let contents = std::fs::read_to_string(proxy_snapshot_path(paths))?;
    Ok(serde_json::from_str(
        contents.trim_start_matches('\u{feff}'),
    )?)
}

fn proxy_snapshot_path(paths: &AppPaths) -> std::path::PathBuf {
    paths.program_data.join("system-proxy-snapshot.json")
}

pub(crate) fn settings_text() -> String {
    let paths = match reactor_paths() {
        Ok(paths) => paths,
        Err(error) => return format!("settings: failed to resolve app paths ({error})"),
    };
    let store = match SettingsStore::load(paths.settings_path()) {
        Ok(store) => store,
        Err(error) => return format!("settings: failed to load ({error})"),
    };
    let settings = store.settings();

    format!(
        "settings\nupdate channel: {:?}\narchitecture: {:?}\nauto check: {}\nauto download: {}\nsystem proxy enabled: {}\nsystem proxy target: {}\nsystem proxy bypass: {}\nrestore on exit: {}\nTUN policy: {:?}",
        settings.updates.channel,
        settings.updates.architecture,
        settings.updates.auto_check,
        settings.updates.auto_download,
        settings.system_proxy.enabled,
        settings.system_proxy.target.server(),
        settings.system_proxy.target.bypass_list(),
        settings.system_proxy.restore_on_exit,
        settings.tun,
    )
}

pub(crate) fn reactor_load_global_override_text() -> String {
    match load_global_override() {
        Ok(contents) if contents.is_empty() => "global override is empty".to_owned(),
        Ok(contents) => contents,
        Err(error) => format!("load global override failed: {error}"),
    }
}

pub(crate) fn reactor_save_global_override_text(contents: &str) -> String {
    match save_global_override(contents) {
        Ok(path) => format!("global override saved\npath: {}", path.display()),
        Err(error) => format!("save global override failed: {error}"),
    }
}

fn load_global_override() -> anyhow::Result<String> {
    let paths = reactor_paths()?;
    let path = global_override_path(&paths);
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(std::fs::read_to_string(path)?)
}

fn save_global_override(contents: &str) -> anyhow::Result<std::path::PathBuf> {
    let paths = reactor_paths()?;
    std::fs::create_dir_all(&paths.program_data)?;
    let path = global_override_path(&paths);
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    } else {
        std::fs::write(&path, contents)?;
    }
    Ok(path)
}
