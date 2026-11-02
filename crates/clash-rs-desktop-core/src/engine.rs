//! App-level connect/disconnect orchestration.
//!
//! The shell (Tauri today) talks to this one type; it owns the core process,
//! the generated config and the OS proxy, and guarantees the user's original
//! proxy settings come back — on disconnect, on core crash, and on the next
//! launch if we ourselves crashed.

use std::{fs, net::TcpListener, path::PathBuf, time::Duration};

use anyhow::Context as _;
use serde_yaml::{Mapping, Value};

use crate::{
    ActiveCore, AppPaths, ClashIpcClient, ClashIpcClientConfig, CoreRuntimeManager, CoreState,
    CoreUpdateManager, ManagedCoreFiles, ProfileStore, SettingsStore, SystemProxyBackend,
    SystemProxySnapshot, WindowsSystemProxy, runtime_options_from_active_profile,
    settings::ConnectMode,
};

/// Points the engine at a local `clash-rs` build instead of downloading one.
pub const CORE_OVERRIDE_ENV: &str = "CLASH_RS_DESKTOP_CORE";

pub struct Engine {
    paths: AppPaths,
    settings: SettingsStore,
    profiles: ProfileStore,
    runtime: Option<CoreRuntimeManager>,
    controller_pipe: String,
    mixed_port: Option<u16>,
}

impl Engine {
    pub fn load(paths: AppPaths) -> anyhow::Result<Self> {
        for dir in [
            &paths.program_data,
            &paths.core_dir,
            &paths.runtime_dir,
            &paths.profiles_dir,
        ] {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }

        let engine = Self {
            settings: SettingsStore::load(paths.settings_path())?,
            profiles: ProfileStore::load(paths.profile_manifest_path())?,
            // Unique per app run so a stale pipe from a crashed run can't
            // shadow the new core's controller.
            controller_pipe: format!(r"\\.\pipe\clash-rs-desktop-{}", std::process::id()),
            runtime: None,
            mixed_port: None,
            paths,
        };

        // A snapshot left behind means we crashed while connected: the OS
        // still points at a dead port. Put the user's settings back first.
        engine.restore_system_proxy()?;
        Ok(engine)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn settings(&self) -> &SettingsStore {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut SettingsStore {
        &mut self.settings
    }

    pub fn profiles(&self) -> &ProfileStore {
        &self.profiles
    }

    pub fn profiles_mut(&mut self) -> &mut ProfileStore {
        &mut self.profiles
    }

    pub fn mixed_port(&self) -> Option<u16> {
        self.mixed_port
    }

    pub fn is_connected(&mut self) -> bool {
        self.runtime
            .as_mut()
            .is_some_and(|runtime| matches!(runtime.state(), CoreState::Running { .. }))
    }

    /// Controller client for the running core, if connected.
    pub fn controller(&self) -> Option<ClashIpcClient> {
        self.runtime.as_ref()?;
        Some(ClashIpcClient::new(ClashIpcClientConfig {
            pipe_path: self.controller_pipe.clone(),
            auth_secret: None,
        }))
    }

    pub fn core_log_tail(&self, count: usize) -> Vec<String> {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.log().tail(count))
            .unwrap_or_default()
    }

    /// The installed core, downloading the latest release on first use.
    pub async fn ensure_core(&self, on_step: &(dyn Fn(&str) + Send + Sync)) -> anyhow::Result<ActiveCore> {
        if let Some(path) = std::env::var_os(CORE_OVERRIDE_ENV).map(PathBuf::from) {
            anyhow::ensure!(path.exists(), "{CORE_OVERRIDE_ENV} points at a missing file");
            return Ok(local_core(path));
        }

        let files = ManagedCoreFiles::from_paths(&self.paths);
        if let Ok(active) = ActiveCore::load(&files.active_metadata)
            && active.binary_path.exists()
        {
            return Ok(active);
        }

        on_step("首次使用，正在下载内核…");
        let manager = CoreUpdateManager::new();
        let check = manager
            .check_latest(&self.settings.settings().updates, None)
            .await
            .context("无法获取内核版本信息，请检查网络")?;
        manager
            .download_and_activate(&check.candidate, &files)
            .await
            .context("内核下载失败，请检查网络")
    }

    pub async fn connect(&mut self, on_step: &(dyn Fn(&str) + Send + Sync)) -> anyhow::Result<()> {
        if self.is_connected() {
            return Ok(());
        }
        // A previous core that died on its own may still be parked here.
        self.runtime = None;

        let core = self.ensure_core(on_step).await?;
        anyhow::ensure!(self.profiles.active_profile().is_some(), "还没有添加订阅");

        on_step("正在连接…");
        let settings = self.settings.settings().clone();
        let port = pick_port(settings.system_proxy.target.port);

        let mut options = runtime_options_from_active_profile(
            self.paths.clone(),
            core,
            &self.profiles,
            Some(self.paths.global_override_path()),
            self.controller_pipe.clone(),
        )?;
        options.extra_runtime_override = shell_settings(port, settings.routing_mode.as_clash());

        let mut runtime = CoreRuntimeManager::new(options);
        let state = tokio::task::block_in_place(|| runtime.start_validated())
            .map_err(|error| anyhow::anyhow!("配置有误，无法启动：{}", first_error_line(&error)))?;
        if let CoreState::NeedsService { .. } = state {
            anyhow::bail!("当前配置需要增强模式");
        }

        if let Err(error) = runtime.wait_until_ready(Duration::from_secs(15)).await {
            let reason = last_error(&runtime.log().tail(30)).unwrap_or_else(|| error.to_string());
            let _ = runtime.stop();
            anyhow::bail!("内核启动失败：{reason}");
        }

        self.runtime = Some(runtime);
        self.mixed_port = Some(port);

        if settings.connect_mode == ConnectMode::SystemProxy
            && let Err(error) = self.apply_system_proxy(port)
        {
            let _ = self.disconnect();
            return Err(error.context("无法设置系统代理"));
        }
        Ok(())
    }

    pub fn disconnect(&mut self) -> anyhow::Result<()> {
        // Restore the network first: even if stopping the core fails, the
        // user must not be left pointing at a dead proxy.
        let restored = self.restore_system_proxy();
        if let Some(mut runtime) = self.runtime.take() {
            runtime.stop()?;
        }
        self.mixed_port = None;
        restored
    }

    /// Poll from the shell. If the core died while we were connected, clean
    /// up and return a user-facing reason.
    pub fn check_health(&mut self) -> Option<String> {
        let runtime = self.runtime.as_mut()?;
        let state = runtime.state();
        if matches!(state, CoreState::Running { .. }) {
            return None;
        }

        let reason = last_error(&runtime.log().tail(30));
        let _ = self.disconnect();
        Some(match (reason, state) {
            (Some(reason), _) => format!("连接意外断开：{reason}"),
            (None, CoreState::Failed { .. }) => "连接意外断开：内核已退出".to_owned(),
            _ => "连接意外断开".to_owned(),
        })
    }

    fn apply_system_proxy(&self, port: u16) -> anyhow::Result<()> {
        let backend = WindowsSystemProxy;
        let snapshot_path = self.paths.system_proxy_snapshot_path();
        // Keep the oldest snapshot: if one exists it holds the user's real
        // settings, while the live values may already be ours.
        if !snapshot_path.exists() {
            let snapshot = backend.snapshot()?;
            fs::write(&snapshot_path, serde_json::to_vec(&snapshot)?)?;
        }

        let mut target = self.settings.settings().system_proxy.target.clone();
        target.port = port;
        backend.apply(&target)
    }

    fn restore_system_proxy(&self) -> anyhow::Result<()> {
        let snapshot_path = self.paths.system_proxy_snapshot_path();
        let Ok(raw) = fs::read(&snapshot_path) else {
            return Ok(());
        };
        let snapshot: SystemProxySnapshot = match serde_json::from_slice(&raw) {
            Ok(snapshot) => snapshot,
            // Unreadable snapshot: turning the proxy off is the safe default.
            Err(_) => SystemProxySnapshot {
                enabled: false,
                server: None,
                bypass: None,
            },
        };
        WindowsSystemProxy.restore(&snapshot)?;
        fs::remove_file(&snapshot_path)?;
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}

fn shell_settings(port: u16, mode: &str) -> Value {
    let mut tun = Mapping::new();
    tun.insert("enable".into(), false.into());

    let mut root = Mapping::new();
    root.insert("mixed-port".into(), u64::from(port).into());
    root.insert("mode".into(), mode.into());
    root.insert("tun".into(), Value::Mapping(tun));
    Value::Mapping(root)
}

/// The preferred port if free, otherwise any free loopback port.
fn pick_port(preferred: u16) -> u16 {
    if TcpListener::bind(("127.0.0.1", preferred)).is_ok() {
        return preferred;
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(preferred)
}

/// The message of the newest ERROR log line, without timestamp/level/module:
/// `26-11-02 15:46:29 ERROR clash_lib::app: bind failed: in use` → `bind failed: in use`.
fn last_error(lines: &[String]) -> Option<String> {
    let line = lines.iter().rev().find(|line| line.contains(" ERROR "))?;
    let rest = line.split_once(" ERROR ")?.1.trim();
    // Drop a leading `module::path:` or `span: file.rs:12:` prefix.
    let message = match rest.split_once(": ") {
        Some((prefix, message)) if !prefix.contains(' ') => message,
        _ => rest,
    };
    Some(message.trim().to_owned()).filter(|m| !m.is_empty())
}

fn first_error_line(error: &anyhow::Error) -> String {
    let text = format!("{error:#}");
    text.lines()
        .map(str::trim)
        .find(|line| line.to_ascii_lowercase().contains("error") && !line.starts_with("clash-rs config validation"))
        .unwrap_or_else(|| text.lines().next().unwrap_or_default())
        .to_owned()
}

fn local_core(path: PathBuf) -> ActiveCore {
    ActiveCore {
        channel: crate::UpdateChannel::Stable,
        release_tag: "local".to_owned(),
        version: "local".to_owned(),
        commit: None,
        asset_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        binary_path: path,
        binary_sha256: None,
        updated_at: time::OffsetDateTime::now_utc(),
    }
}

#[cfg(test)]
mod tests {
    use super::{last_error, pick_port, shell_settings};

    #[test]
    fn last_error_ignores_info_and_strips_prefixes() {
        let lines = [
            "26-11-02 15:46:29 ERROR clash_lib::app::inbound: bind failed: address in use",
            "26-11-02 15:46:30 INFO dispatch_stream: matched [TCP] a -> b",
        ]
        .map(str::to_owned);
        assert_eq!(last_error(&lines).as_deref(), Some("bind failed: address in use"));

        let info_only = ["26-11-02 15:46:30 INFO all good".to_owned()];
        assert_eq!(last_error(&info_only), None);
    }

    #[test]
    fn shell_settings_force_ports_mode_and_tun_off() {
        let value = shell_settings(7891, "global");
        assert_eq!(value["mixed-port"], serde_yaml::Value::from(7891u64));
        assert_eq!(value["mode"], serde_yaml::Value::from("global"));
        assert_eq!(value["tun"]["enable"], serde_yaml::Value::from(false));
    }

    #[test]
    fn pick_port_avoids_a_taken_port() {
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        assert_ne!(pick_port(port), port);
    }
}
