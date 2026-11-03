//! App-level connect/disconnect orchestration.
//!
//! The shell (Tauri) talks to this one type. It owns the core — either as a
//! local child process (system-proxy mode, and TUN on Linux) or through the
//! privileged Windows service (TUN) — plus the generated config and the OS
//! proxy, and guarantees the user's original proxy settings come back: on
//! disconnect, on core crash, and on the next launch if we ourselves crashed.

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context as _;
use serde::Serialize;
use serde_yaml::Value;

use crate::{
    ActiveCore, AppPaths, ClashIpcClient, ClashIpcClientConfig, ClashProcessManager,
    CoreLaunchRequest, CoreState, CoreUpdateManager, ManagedCoreFiles, ProfileStore,
    SettingsStore, SystemProxyBackend, SystemProxySnapshot,
    elevate::{Elevated, run_elevated},
    generate::{ConfigInputs, generate},
    service::{Request, Response, SERVICE_VERSION, ServiceClient},
    settings::ConnectMode,
    system_proxy::PlatformSystemProxy,
};

/// Points the engine at a local `clash-rs` build instead of downloading one.
pub const CORE_OVERRIDE_ENV: &str = "CLASH_RS_DESKTOP_CORE";

/// Returned (inside `anyhow::Error`) when TUN needs a one-time privileged
/// install: the Windows service, or Linux capabilities on the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NeedsServiceInstall {
    Missing,
    Outdated,
}

impl std::fmt::Display for NeedsServiceInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "增强模式需要先完成一次授权安装",
            Self::Outdated => "增强模式组件需要更新",
        })
    }
}

impl std::error::Error for NeedsServiceInstall {}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreUpdateInfo {
    pub current: Option<String>,
    pub latest: String,
    pub available: bool,
}

enum Backend {
    /// `bool`: whether this local core runs TUN (Linux).
    Local(ClashProcessManager, bool),
    /// The core runs inside the Windows service.
    Service,
}

pub struct Engine {
    paths: AppPaths,
    settings: SettingsStore,
    profiles: ProfileStore,
    backend: Option<Backend>,
    controller: Option<ClashIpcClient>,
    mixed_port: Option<u16>,
    service: ServiceClient,
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
            backend: None,
            controller: None,
            mixed_port: None,
            service: ServiceClient::default(),
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

    pub fn is_connected(&self) -> bool {
        self.backend.is_some()
    }

    /// Which mode the live connection uses (may differ from settings until
    /// the next reconnect).
    pub fn connected_mode(&self) -> Option<ConnectMode> {
        match self.backend {
            Some(Backend::Local(_, true)) | Some(Backend::Service) => Some(ConnectMode::Tun),
            Some(Backend::Local(_, false)) => Some(ConnectMode::SystemProxy),
            None => None,
        }
    }

    /// Controller client for the running core, if connected.
    pub fn controller(&self) -> Option<ClashIpcClient> {
        self.controller.clone()
    }

    pub async fn core_log_tail(&self, count: usize) -> Vec<String> {
        match &self.backend {
            Some(Backend::Local(process, _)) => process.log().tail(count),
            Some(Backend::Service) => match self.service.send(&Request::Logs { count }).await {
                Ok(Response::Logs { lines }) => lines,
                _ => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    // ---- core binary --------------------------------------------------------

    fn installed_core(&self) -> Option<ActiveCore> {
        if let Some(path) = std::env::var_os(CORE_OVERRIDE_ENV).map(PathBuf::from) {
            return path.exists().then(|| local_core(path));
        }
        let files = ManagedCoreFiles::from_paths(&self.paths);
        ActiveCore::load(&files.active_metadata)
            .ok()
            .filter(|active| active.binary_path.exists())
    }

    /// The installed core, downloading the latest release on first use.
    pub async fn ensure_core(&self, on_step: &(dyn Fn(&str) + Send + Sync)) -> anyhow::Result<ActiveCore> {
        if let Some(core) = self.installed_core() {
            return Ok(core);
        }
        anyhow::ensure!(
            std::env::var_os(CORE_OVERRIDE_ENV).is_none(),
            "{CORE_OVERRIDE_ENV} points at a missing file"
        );

        on_step("首次使用，正在下载内核…");
        let manager = CoreUpdateManager::new();
        let check = manager
            .check_latest(&self.settings.settings().updates, None)
            .await
            .context("无法获取内核版本信息，请检查网络")?;
        manager
            .download_and_activate(&check.candidate, &ManagedCoreFiles::from_paths(&self.paths))
            .await
            .context("内核下载失败，请检查网络")
    }

    pub fn core_version(&self) -> Option<String> {
        self.installed_core().map(|core| core.version)
    }

    pub async fn check_core_update(&self) -> anyhow::Result<CoreUpdateInfo> {
        let current = self.installed_core();
        let check = CoreUpdateManager::new()
            .check_latest(&self.settings.settings().updates, current.as_ref())
            .await
            .context("无法获取内核版本信息，请检查网络")?;
        let latest = crate::binary_update::version_from_tag(&check.candidate.release_tag);
        Ok(CoreUpdateInfo {
            available: current.as_ref().is_none_or(|core| core.version != latest),
            current: current.map(|core| core.version),
            latest,
        })
    }

    /// Download the latest core; the caller reconnects to start using it.
    pub async fn update_core(&mut self) -> anyhow::Result<String> {
        anyhow::ensure!(
            std::env::var_os(CORE_OVERRIDE_ENV).is_none(),
            "正在使用 {CORE_OVERRIDE_ENV} 指定的内核"
        );
        let settings = self.settings.settings().updates.clone();
        let manager = CoreUpdateManager::new();
        let check = manager.check_latest(&settings, None).await?;
        let active = manager
            .download_and_activate(&check.candidate, &ManagedCoreFiles::from_paths(&self.paths))
            .await
            .context("内核下载失败，请检查网络")?;
        // Keep the service's private copy in step, if it is installed.
        if self.service_version().await.is_some() {
            let _ = self
                .service
                .send(&Request::UpdateCore {
                    channel: settings.channel,
                })
                .await;
        }
        Ok(active.version)
    }

    // ---- config ----------------------------------------------------------------

    fn build_config(&self, port: u16, tun: bool, controller_ipc: Option<&str>) -> anyhow::Result<Value> {
        let profile = self
            .profiles
            .active_profile()
            .ok_or_else(|| anyhow::anyhow!("还没有添加订阅"))?;
        let subscription = read_yaml(&profile.source_path)?
            .ok_or_else(|| anyhow::anyhow!("订阅文件丢失，请更新订阅"))?;

        let mut overrides = Vec::new();
        for path in [Some(self.paths.global_override_path()), profile.profile_override_path.clone()]
            .into_iter()
            .flatten()
        {
            if let Some(layer) = read_yaml(&path).context("自定义配置格式有误")? {
                overrides.push(layer);
            }
        }

        let settings = self.settings.settings();
        Ok(generate(ConfigInputs {
            subscription,
            overrides,
            custom_rules: &settings.custom_rules,
            mixed_port: port,
            mode: settings.routing_mode.as_clash(),
            tun,
            controller_ipc,
            controller_http: None,
        }))
    }

    fn local_launch(&self, core: &ActiveCore, config_path: &Path) -> CoreLaunchRequest {
        CoreLaunchRequest {
            binary_path: core.binary_path.clone(),
            runtime_dir: self.paths.runtime_dir.clone(),
            config_path: config_path.to_owned(),
            core_pipe_path: local_controller_path(&self.paths),
        }
    }

    /// Write `config` to the live config path, validating it first so a bad
    /// config never replaces a good one.
    fn write_local_config(&self, core: &ActiveCore, config: &Value) -> anyhow::Result<PathBuf> {
        let path = self.paths.runtime_dir.join("generated.yaml");
        let staging = self.paths.runtime_dir.join("generated.yaml.new");
        fs::write(&staging, serde_yaml::to_string(config)?)?;
        let check = self.local_launch(core, &staging);
        if let Err(error) = tokio::task::block_in_place(|| check.validate_config()) {
            let _ = fs::remove_file(&staging);
            anyhow::bail!("配置有误，无法启动：{}", first_error_line(&error));
        }
        fs::rename(&staging, &path)?;
        Ok(path)
    }

    // ---- connect ---------------------------------------------------------------

    pub async fn connect(&mut self, on_step: &(dyn Fn(&str) + Send + Sync)) -> anyhow::Result<()> {
        if self.is_connected() {
            return Ok(());
        }
        anyhow::ensure!(self.profiles.active_profile().is_some(), "还没有添加订阅");

        let core = self.ensure_core(on_step).await?;
        on_step("正在连接…");
        let port = pick_port(self.settings.settings().system_proxy.target.port);

        match self.settings.settings().connect_mode {
            ConnectMode::SystemProxy => {
                self.start_local(&core, port, false).await?;
                if let Err(error) = self.apply_system_proxy(port) {
                    let _ = self.disconnect().await;
                    return Err(error.context("无法设置系统代理"));
                }
            }
            // The privileged service owns TUN on every platform: on Linux,
            // capabilities on the core alone aren't enough (clash-rs runs
            // `ip` to set up routes, and caps don't pass to that child).
            ConnectMode::Tun => self.start_service(port).await?,
        }
        self.mixed_port = Some(port);
        Ok(())
    }

    async fn start_local(&mut self, core: &ActiveCore, port: u16, tun: bool) -> anyhow::Result<()> {
        let ipc = local_controller_path(&self.paths);
        let config = self.build_config(port, tun, Some(&ipc))?;
        let config_path = self.write_local_config(core, &config)?;

        let mut process = ClashProcessManager::new();
        process.start_direct(&self.local_launch(core, &config_path))?;
        let client = controller_client(&ipc);
        if let Err(error) = wait_ready(&client, Duration::from_secs(15)).await {
            let reason = process.log().last_error().unwrap_or_else(|| error.to_string());
            let _ = process.stop();
            anyhow::bail!("内核启动失败：{reason}");
        }
        if tun && let Err(reason) = process.log().wait_for_tun(Duration::from_secs(5)).await {
            let _ = process.stop();
            if reason.contains("permission denied") {
                // Capabilities missing or not honoured: ask to authorise again.
                return Err(NeedsServiceInstall::Missing.into());
            }
            anyhow::bail!("增强模式启动失败：{reason}");
        }
        self.backend = Some(Backend::Local(process, tun));
        self.controller = Some(client);
        Ok(())
    }

    async fn service_version(&self) -> Option<String> {
        match self.service.send(&Request::Ping).await {
            Ok(Response::Pong { version }) => Some(version),
            _ => None,
        }
    }

    async fn start_service(&mut self, port: u16) -> anyhow::Result<()> {
        match self.service_version().await {
            None => return Err(NeedsServiceInstall::Missing.into()),
            Some(version) if version != SERVICE_VERSION => {
                return Err(NeedsServiceInstall::Outdated.into());
            }
            Some(_) => {}
        }
        let config = serde_yaml::to_string(&self.build_config(port, true, None)?)?;
        match self.service.send(&Request::Start { config }).await? {
            Response::Ok => {}
            Response::Error { message } => anyhow::bail!("增强模式启动失败：{message}"),
            other => anyhow::bail!("unexpected service response: {other:?}"),
        }
        self.backend = Some(Backend::Service);
        self.controller = Some(controller_client(crate::service::CORE_PIPE));
        Ok(())
    }

    /// The service may still be running a TUN core from before the app
    /// restarted; pick it up instead of starting a second one.
    pub async fn adopt_running_service(&mut self) -> bool {
        if self.is_connected() {
            return false;
        }
        if self.service_version().await.as_deref() != Some(SERVICE_VERSION) {
            return false;
        }
        match self.service.send(&Request::Status).await {
            Ok(Response::Status { status }) if status.running => {
                self.backend = Some(Backend::Service);
                self.controller = Some(controller_client(crate::service::CORE_PIPE));
                true
            }
            _ => false,
        }
    }

    pub async fn disconnect(&mut self) -> anyhow::Result<()> {
        // Restore the network first: even if stopping the core fails, the
        // user must not be left pointing at a dead proxy.
        let restored = self.restore_system_proxy();
        self.controller = None;
        self.mixed_port = None;
        match self.backend.take() {
            Some(Backend::Local(mut process, _)) => {
                process.stop()?;
            }
            Some(Backend::Service) => {
                if let Response::Error { message } = self.service.send(&Request::Stop).await? {
                    anyhow::bail!(message);
                }
            }
            None => {}
        }
        restored
    }

    /// Apply config changes to the running core without dropping the
    /// connection. No-op when disconnected.
    pub async fn reload(&mut self) -> anyhow::Result<()> {
        let Some(port) = self.mixed_port else {
            return Ok(());
        };
        match &self.backend {
            Some(Backend::Local(..)) => {
                let core = self.installed_core().context("内核文件丢失")?;
                let tun = self.connected_mode() == Some(ConnectMode::Tun);
                let config = self.build_config(port, tun, Some(&local_controller_path(&self.paths)))?;
                self.write_local_config(&core, &config)?;
                // `{}` = reload the startup config file we just replaced. A
                // timeout surfaces as an error; the shell then reconnects.
                let client = self.controller.clone().context("尚未连接")?;
                tokio::time::timeout(
                    Duration::from_secs(10),
                    client.put_json::<_, serde_json::Value>(
                        "/configs?force=true",
                        &serde_json::json!({}),
                    ),
                )
                .await
                .context("内核重新加载配置超时")??;
            }
            Some(Backend::Service) => {
                let config = serde_yaml::to_string(&self.build_config(port, true, None)?)?;
                if let Response::Error { message } = self.service.send(&Request::Reload { config }).await? {
                    anyhow::bail!("配置有误：{message}");
                }
            }
            None => {}
        }
        Ok(())
    }

    /// Poll from the shell. If the core died while we were connected, clean
    /// up and return a user-facing reason.
    pub async fn check_health(&mut self) -> Option<String> {
        let reason = match self.backend.as_mut()? {
            Backend::Local(process, _) => match process.state() {
                CoreState::Running { .. } => return None,
                CoreState::Failed { message } => {
                    process.log().last_error().unwrap_or(format!("内核已退出（{message}）"))
                }
                _ => process.log().last_error().unwrap_or_else(|| "内核已退出".to_owned()),
            },
            Backend::Service => match self.service.send(&Request::Status).await {
                Ok(Response::Status { status }) if status.running => return None,
                Ok(Response::Status { status }) => {
                    status.last_error.unwrap_or_else(|| "内核已退出".to_owned())
                }
                // The service itself is gone (stopped/uninstalled).
                _ => "增强模式服务已停止".to_owned(),
            },
        };
        let _ = self.disconnect().await;
        Some(format!("连接意外断开：{reason}"))
    }

    // ---- privileged install ----------------------------------------------------

    /// One-time privileged setup for TUN. `service_exe` is the service binary
    /// shipped with the app (Windows only).
    pub async fn install_tun_support(&self, service_exe: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(service_exe.exists(), "找不到增强模式组件：{}", service_exe.display());
        let core = self.ensure_core(&|_| {}).await?;
        let (exe, args) = if cfg!(windows) {
            (
                service_exe.to_owned(),
                format!("--install --core \"{}\"", core.binary_path.display()),
            )
        } else {
            // An AppImage's files live on a per-user FUSE mount root can't
            // read; hand pkexec a copy in our own data dir instead.
            let staged = self.paths.runtime_dir.join("clash-rs-desktop-service");
            fs::copy(service_exe, &staged).context("无法准备增强模式组件")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
            }
            // pkexec passes argv verbatim (split on whitespace by run_elevated).
            (staged, format!("--install --core {}", core.binary_path.display()))
        };
        match tokio::task::spawn_blocking(move || run_elevated(&exe, &args)).await?? {
            Elevated::Exited(0) => {}
            Elevated::Cancelled => anyhow::bail!("已取消授权"),
            Elevated::Exited(code) => anyhow::bail!("安装失败（错误码 {code}）"),
        }
        // The service starts asynchronously; give it a moment to listen.
        for _ in 0..50 {
            if self.service_version().await.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        anyhow::bail!("增强模式服务没有启动")
    }

    // ---- system proxy ----------------------------------------------------------

    fn apply_system_proxy(&self, port: u16) -> anyhow::Result<()> {
        let backend = PlatformSystemProxy;
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
        PlatformSystemProxy.restore(&snapshot)?;
        fs::remove_file(&snapshot_path)?;
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Best effort without async: restore the proxy and stop a local core.
        // A service-owned TUN core keeps running and is re-adopted next launch.
        let _ = self.restore_system_proxy();
        if let Some(Backend::Local(mut process, _)) = self.backend.take() {
            let _ = process.stop();
        }
    }
}

fn controller_client(endpoint: &str) -> ClashIpcClient {
    ClashIpcClient::new(ClashIpcClientConfig {
        pipe_path: endpoint.to_owned(),
        auth_secret: None,
    })
}

/// Unique per app run so a stale endpoint from a crashed run can't shadow
/// the new core's controller.
fn local_controller_path(paths: &AppPaths) -> String {
    if cfg!(windows) {
        format!(r"\\.\pipe\choclite-{}", std::process::id())
    } else {
        paths
            .runtime_dir
            .join(format!("controller-{}.sock", std::process::id()))
            .display()
            .to_string()
    }
}

async fn wait_ready(client: &ClashIpcClient, timeout: Duration) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    loop {
        match client.get_version().await {
            Ok(_) => return Ok(()),
            Err(error) if started.elapsed() > timeout => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

fn read_yaml(path: &Path) -> anyhow::Result<Option<Value>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(None);
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_yaml::from_str(&text)?))
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

/// The useful part of `clash-rs --test-config` output, e.g.
/// `configuration file c.yaml test failed: invalid config: invalid config:
/// unsupported rule type: X` → `unsupported rule type: X`.
fn first_error_line(error: &anyhow::Error) -> String {
    let text = format!("{error:#}");
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("clash-rs config validation")
                && !matches!(*line, "stdout:" | "stderr:")
        })
        .collect();
    let line = lines
        .iter()
        .find(|line| line.contains("test failed:"))
        .or_else(|| lines.last())
        .copied()
        .unwrap_or("未知错误");
    let detail = line
        .rsplit_once("invalid config: ")
        .or_else(|| line.rsplit_once("test failed: "))
        .map_or(line, |(_, rest)| rest);
    detail.trim_start_matches(": ").trim().to_owned()
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
    use super::{first_error_line, pick_port};

    #[test]
    fn validation_errors_are_trimmed_to_the_cause() {
        let error = anyhow::anyhow!(
            "clash-rs config validation failed with exit code: 1\nstdout:\n\nstderr:\nconfiguration file C:\\x\\c.yaml test failed: invalid config: invalid config: unsupported rule type: BOGUS-RULE"
        );
        assert_eq!(first_error_line(&error), "unsupported rule type: BOGUS-RULE");

        let yaml_error =
            anyhow::anyhow!("clash-rs config validation failed\nstderr:\nrules:\n  - x\n\n: duplicate entry with key \"rules\"");
        assert_eq!(first_error_line(&yaml_error), "duplicate entry with key \"rules\"");
    }

    #[test]
    fn pick_port_avoids_a_taken_port() {
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        assert_ne!(pick_port(port), port);
    }

    #[test]
    fn last_error_ignores_info_and_strips_prefixes() {
        let lines = [
            "26-11-02 15:46:29 ERROR clash_lib::app::inbound: bind failed: address in use",
            "26-11-02 15:46:30 INFO dispatch_stream: matched [TCP] a -> b",
        ]
        .map(str::to_owned);
        assert_eq!(
            crate::process::last_error(&lines).as_deref(),
            Some("bind failed: address in use")
        );
        let info_only = ["26-11-02 15:46:30 INFO all good".to_owned()];
        assert_eq!(crate::process::last_error(&info_only), None);
    }
}
