//! The privileged service behind "增强模式" (TUN): protocol, client, and the
//! supervisor it runs.
//!
//! Security model — the service runs as LocalSystem and any local user can
//! reach its pipe, so it never trusts the caller with anything executable:
//! - it only launches the core binary from its own admin-only install dir;
//! - callers send config *content*, which is sanitised (no controller/UI
//!   overrides, no file paths outside the service's data dir) before use;
//! - the core runs in a kill-on-close job owned by the service.

use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::{
    ClashIpcClient, ClashIpcClientConfig, ClashProcessManager, CoreLaunchRequest, CoreState,
    UpdateChannel, i18n::t, tr,
};

pub const SERVICE_NAME: &str = "ChocliteService";
pub const SERVICE_DISPLAY_NAME: &str = "Choclite Service";
/// The service's request endpoint: a named pipe on Windows, a unix socket
/// on Linux (served by the systemd unit `choclite.service`).
#[cfg(windows)]
pub const SERVICE_PIPE: &str = r"\\.\pipe\choclite-service";
#[cfg(unix)]
pub const SERVICE_PIPE: &str = "/run/choclite/service.sock";
/// Controller endpoint of the service-owned core.
#[cfg(windows)]
pub const CORE_PIPE: &str = r"\\.\pipe\choclite-core";
#[cfg(unix)]
pub const CORE_PIPE: &str = "/run/choclite/core.sock";
/// Bump the suffix whenever the service's behaviour changes; an app that
/// finds an older service asks for one UAC/pkexec prompt to update it.
pub const SERVICE_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+svc2");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Request {
    Ping,
    /// Start (or restart) the core with this config.
    Start {
        config: String,
    },
    /// Apply a new config to the running core without dropping the tunnel.
    Reload {
        config: String,
    },
    Stop,
    Status,
    Logs {
        count: usize,
    },
    /// Download the latest core release into the service's own directory.
    UpdateCore {
        channel: UpdateChannel,
    },
    /// Clear the OS resolver cache (a diagnostics fix; needs privileges).
    FlushDns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Response {
    Ok,
    Pong { version: String },
    Status { status: ServiceStatus },
    Logs { lines: Vec<String> },
    Error { message: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub running: bool,
    pub core_version: Option<String>,
    /// Why the core stopped, if it died on its own.
    pub last_error: Option<String>,
}

/// Where the service keeps its files.
#[derive(Debug, Clone)]
pub struct ServicePaths {
    /// Admin-only: service exe, `clash-rs.exe`, `wintun.dll`.
    pub install_dir: PathBuf,
    /// Admin-only (ACL set at install): config, provider caches, geodata.
    pub data_dir: PathBuf,
}

impl ServicePaths {
    #[cfg(windows)]
    pub fn system() -> Self {
        let program_files = std::env::var_os("ProgramFiles")
            .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
        let program_data = std::env::var_os("ProgramData")
            .map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
        Self {
            install_dir: program_files.join("Choclite").join("Service"),
            data_dir: program_data.join("Choclite").join("Service"),
        }
    }

    /// Root-owned on Linux: binaries 0755 under /opt, state 0700 under /var/lib.
    #[cfg(unix)]
    pub fn system() -> Self {
        Self {
            install_dir: PathBuf::from("/opt/choclite/service"),
            data_dir: PathBuf::from("/var/lib/choclite/service"),
        }
    }

    pub fn service_binary(&self) -> PathBuf {
        self.install_dir.join(format!(
            "clash-rs-desktop-service{}",
            std::env::consts::EXE_SUFFIX
        ))
    }

    pub fn core_binary(&self) -> PathBuf {
        self.install_dir
            .join(format!("clash-rs{}", std::env::consts::EXE_SUFFIX))
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("config.yaml")
    }
}

/// Make a caller-supplied config safe to run as LocalSystem.
pub fn sanitize_config(config: &mut Value) -> anyhow::Result<()> {
    let map = config
        .as_mapping_mut()
        .context(t("配置格式不正确", "The config is malformed"))?;

    // The service decides how the core is controlled and served.
    for key in [
        "external-controller",
        "external-controller-ipc",
        "external-controller-pipe",
        "external-controller-unix",
        "external-ui",
        "secret",
    ] {
        map.remove(key);
    }

    // Providers cache downloads to `path`: pin them inside the data dir.
    for section in ["proxy-providers", "rule-providers"] {
        let Some(Value::Mapping(providers)) = map.get_mut(section) else {
            continue;
        };
        for (index, (name, provider)) in providers.iter_mut().enumerate() {
            let Some(provider) = provider.as_mapping_mut() else {
                continue;
            };
            if provider.get("type").and_then(Value::as_str) == Some("file") {
                anyhow::bail!(tr!(
                    "增强模式不支持读取本地文件的规则或节点集（{}）",
                    "Enhanced Mode can't use rule or server sets from local files ({})",
                    name.as_str().unwrap_or("?")
                ));
            }
            provider.insert(
                "path".into(),
                format!("./providers/{section}-{index}.yaml").into(),
            );
        }
    }

    reject_paths(config, "")?;
    Ok(())
}

/// Keys whose string values the core treats as file paths. (`path` is not
/// here: proxies use it for URL paths like `ws-opts.path: /ws`; provider
/// paths are rewritten above instead.)
const PATH_KEYS: [&str; 11] = [
    "file",
    "certificate",
    "private-key",
    "cert",
    "ca",
    "ca-file",
    "mmdb",
    "asn-mmdb",
    "geosite",
    "geoip",
    "cache-file",
];

fn reject_paths(value: &Value, key: &str) -> anyhow::Result<()> {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                reject_paths(v, k.as_str().unwrap_or_default())?;
            }
        }
        Value::Sequence(items) => {
            for item in items {
                reject_paths(item, key)?;
            }
        }
        Value::String(text) if PATH_KEYS.contains(&key) && escapes_data_dir(text, key) => {
            anyhow::bail!(tr!(
                "增强模式下配置不能引用其他目录的文件（{key}: {text}）",
                "In Enhanced Mode the config can't reference files in other folders ({key}: {text})"
            ));
        }
        _ => {}
    }
    Ok(())
}

/// Absolute (drive, UNC, rooted) or parent-relative paths. `private-key` may
/// hold base64 key material, which can start with `/` but never contains
/// `\`, `:` or a `..` segment, so rooted-`/` is only checked for other keys.
fn escapes_data_dir(text: &str, key: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    drive
        || text.starts_with('\\')
        || (text.starts_with('/') && key != "private-key")
        || text.split(['/', '\\']).any(|segment| segment == "..")
}

/// Runs inside the service. One per service process.
pub struct Supervisor {
    paths: ServicePaths,
    process: ClashProcessManager,
    core_version: Option<String>,
    last_config: Option<String>,
}

impl Supervisor {
    pub fn new(paths: ServicePaths) -> Self {
        // Users can read (not write) data_dir, so they can open these logs.
        let log = crate::CoreLog::with_file(&paths.data_dir.join("logs").join("core.log"));
        Self {
            paths,
            process: ClashProcessManager::with_log(log),
            core_version: None,
            last_config: None,
        }
    }

    pub async fn handle(&mut self, request: Request) -> Response {
        let result = match request {
            Request::Ping => {
                return Response::Pong {
                    version: SERVICE_VERSION.to_owned(),
                };
            }
            Request::Status => {
                return Response::Status {
                    status: self.status(),
                };
            }
            Request::Logs { count } => {
                return Response::Logs {
                    lines: self.process.log().tail(count.min(500)),
                };
            }
            Request::Start { config } => self.start(&config).await,
            Request::Reload { config } => self.reload(&config).await,
            Request::Stop => self.process.stop().map(|_| {
                // Cached fake-IP answers (198.18.x.x) would break the first
                // seconds after disconnecting.
                flush_os_dns_cache();
            }),
            Request::UpdateCore { channel } => self.update_core(channel).await,
            Request::FlushDns => {
                flush_os_dns_cache();
                Ok(())
            }
        };
        match result {
            Ok(()) => Response::Ok,
            Err(error) => Response::Error {
                message: format!("{error:#}"),
            },
        }
    }

    fn status(&mut self) -> ServiceStatus {
        let state = self.process.state();
        ServiceStatus {
            running: matches!(state, CoreState::Running { .. }),
            core_version: self.core_version.clone(),
            last_error: match state {
                CoreState::Failed { message } => {
                    Some(self.process.log().last_error().unwrap_or(message))
                }
                _ => None,
            },
        }
    }

    fn launch_request(&self) -> CoreLaunchRequest {
        CoreLaunchRequest {
            binary_path: self.paths.core_binary(),
            runtime_dir: self.paths.data_dir.clone(),
            config_path: self.paths.config_path(),
            core_pipe_path: CORE_PIPE.to_owned(),
        }
    }

    /// Sanitise, write and validate a config; returns once it's on disk.
    fn install_config(&self, config: &str) -> anyhow::Result<()> {
        let mut value: Value =
            serde_yaml::from_str(config).context(t("配置格式不正确", "The config is malformed"))?;
        sanitize_config(&mut value)?;
        std::fs::create_dir_all(&self.paths.data_dir)?;

        let path = self.paths.config_path();
        let staging = path.with_extension("yaml.new");
        std::fs::write(&staging, serde_yaml::to_string(&value)?)?;
        let check = CoreLaunchRequest {
            config_path: staging.clone(),
            ..self.launch_request()
        };
        if let Err(error) = tokio::task::block_in_place(|| check.validate_config()) {
            let _ = std::fs::remove_file(&staging);
            return Err(error);
        }
        std::fs::rename(&staging, &path)?;
        Ok(())
    }

    async fn start(&mut self, config: &str) -> anyhow::Result<()> {
        self.install_config(config)?;
        self.process.stop()?;
        clear_stale_routes();
        self.process.start_direct(&self.launch_request())?;

        let ready = match wait_for_controller(Duration::from_secs(15)).await {
            // clash-rs keeps running if TUN fails; that is still a failure here.
            Ok(version) => match self
                .process
                .log()
                .wait_for_tun(Duration::from_secs(5))
                .await
            {
                Ok(()) => Ok(version),
                Err(reason) => Err(anyhow::anyhow!(reason)),
            },
            Err(error) => Err(error),
        };
        match ready {
            Ok(version) => {
                open_core_endpoint();
                // Lookups that failed while the tunnel came up are cached by
                // the OS and would keep failing for a while; drop them.
                flush_os_dns_cache();
                self.core_version = Some(version);
                self.last_config = Some(config.to_owned());
                Ok(())
            }
            Err(error) => {
                let reason = self.process.log().last_error();
                let _ = self.process.stop();
                Err(reason.map_or(error, |reason| anyhow::anyhow!(reason)))
            }
        }
    }

    async fn reload(&mut self, config: &str) -> anyhow::Result<()> {
        if !matches!(self.process.state(), CoreState::Running { .. }) {
            return self.start(config).await;
        }
        self.install_config(config)?;
        // `{}` = reload the startup config file we just replaced. Bounded:
        // a hung reload must not wedge the service; fall back to a restart.
        let client = controller();
        let body = serde_json::json!({});
        let reload = client.put_json::<_, serde_json::Value>("/configs?force=true", &body);
        match tokio::time::timeout(Duration::from_secs(10), reload).await {
            Ok(result) => {
                result?;
            }
            Err(_) => return self.start(config).await,
        }
        open_core_endpoint();
        self.last_config = Some(config.to_owned());
        Ok(())
    }

    async fn update_core(&mut self, channel: UpdateChannel) -> anyhow::Result<()> {
        let manager = crate::CoreUpdateManager::new();
        let settings = crate::CoreUpdateSettings {
            channel,
            ..crate::CoreUpdateSettings::default()
        };
        let check = manager.check_latest(&settings, None).await?;
        let files = crate::ManagedCoreFiles {
            current_binary: self.paths.core_binary(),
            rollback_binary: self
                .paths
                .install_dir
                .join(format!("clash-rs.rollback{}", std::env::consts::EXE_SUFFIX)),
            wintun_dll: self.paths.install_dir.join("wintun.dll"),
            active_metadata: self.paths.install_dir.join("core.json"),
        };
        // A running exe can be renamed but not overwritten; activation
        // renames the old one aside, so this is safe while running.
        manager
            .download_and_activate(&check.candidate, &files)
            .await?;

        if matches!(self.process.state(), CoreState::Running { .. })
            && let Some(config) = self.last_config.clone()
        {
            self.start(&config).await?;
        }
        Ok(())
    }
}

/// Clear the OS resolver cache (needs the service's privileges).
fn flush_os_dns_cache() {
    use std::process::{Command, Stdio};
    let mut command = if cfg!(windows) {
        let mut command = Command::new("ipconfig");
        command.arg("/flushdns");
        command
    } else {
        let mut command = Command::new("resolvectl");
        command.arg("flush-caches");
        command
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
}

/// Linux: a core that was killed (crash, SIGKILL) leaves its policy-routing
/// rules behind, and clash-rs's route setup then fails with "File exists".
/// Remove what it installs (`route-table` defaults to 2468) before starting.
fn clear_stale_routes() {
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        let ip = |args: &[&str]| {
            Command::new("ip")
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        for family in ["-4", "-6"] {
            // Each `del` removes one matching rule; loop until none is left.
            for rule in [
                &["rule", "del", "table", "main", "suppress_prefixlength", "0"][..],
                &["rule", "del", "table", "2468"][..],
            ] {
                let args: Vec<&str> = std::iter::once(family)
                    .chain(rule.iter().copied())
                    .collect();
                for _ in 0..16 {
                    if !ip(&args) {
                        break;
                    }
                }
            }
            ip(&[family, "route", "flush", "table", "2468"]);
        }
    }
}

/// Linux: the root-run core creates its controller socket with root's umask,
/// so the user's app couldn't connect. (Windows: clash-rs sets the pipe DACL.)
fn open_core_endpoint() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(CORE_PIPE, std::fs::Permissions::from_mode(0o666));
    }
}

fn controller() -> ClashIpcClient {
    ClashIpcClient::new(ClashIpcClientConfig {
        pipe_path: CORE_PIPE.to_owned(),
        auth_secret: None,
    })
}

async fn wait_for_controller(timeout: Duration) -> anyhow::Result<String> {
    let started = std::time::Instant::now();
    let client = controller();
    loop {
        match client.get_version().await {
            Ok(version) => return Ok(version.version),
            Err(error) if started.elapsed() > timeout => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_millis(150)).await,
        }
    }
}

fn request_timeout(request: &Request) -> Duration {
    match request {
        Request::UpdateCore { .. } => Duration::from_secs(300),
        Request::Start { .. } | Request::Reload { .. } => Duration::from_secs(45),
        _ => Duration::from_secs(10),
    }
}

/// Talks to the service over its pipe/socket: one request per connection,
/// one JSON line each way.
#[derive(Debug, Clone)]
pub struct ServiceClient {
    endpoint: String,
}

impl Default for ServiceClient {
    fn default() -> Self {
        Self::new(SERVICE_PIPE)
    }
}

impl ServiceClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    pub async fn send(&self, request: &Request) -> anyhow::Result<Response> {
        tokio::time::timeout(request_timeout(request), self.round_trip(request))
            .await
            .map_err(|_| {
                anyhow::anyhow!(t(
                    "增强模式服务没有响应",
                    "The Enhanced Mode service isn't responding"
                ))
            })?
    }

    async fn round_trip(&self, request: &Request) -> anyhow::Result<Response> {
        use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

        let mut stream = BufReader::new(transport::connect(&self.endpoint).await?);
        let mut line = serde_json::to_string(request)?;
        line.push('\n');
        stream.get_mut().write_all(line.as_bytes()).await?;
        stream.get_mut().flush().await?;

        let mut reply = String::new();
        stream.read_line(&mut reply).await?;
        anyhow::ensure!(
            !reply.is_empty(),
            "{}",
            t(
                "增强模式服务断开了连接",
                "The Enhanced Mode service closed the connection"
            )
        );
        Ok(serde_json::from_str(reply.trim())?)
    }
}

/// Serve clients until `shutdown` flips; stops the core on the way out.
pub async fn serve(
    endpoint: &str,
    supervisor: Supervisor,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    // The log is shared (not behind the lock) so it stays readable even
    // while a slow start/reload holds the supervisor.
    let log = supervisor.process.log().clone();
    let supervisor = std::sync::Arc::new(tokio::sync::Mutex::new(supervisor));
    let mut listener = transport::Listener::bind(endpoint)?;
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            client = listener.accept() => {
                let client = client?;
                let supervisor = supervisor.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    if let Err(error) = serve_client(client, supervisor, log).await {
                        eprintln!("client error: {error:#}");
                    }
                });
            }
        }
    }
    let _ = supervisor.lock().await.handle(Request::Stop).await;
    Ok(())
}

async fn serve_client<S>(
    client: S,
    supervisor: std::sync::Arc<tokio::sync::Mutex<Supervisor>>,
    log: crate::CoreLog,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let mut stream = BufReader::new(client);
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_line(&mut line))
        .await
        .context("client sent nothing")??;

    let response = match serde_json::from_str::<Request>(line.trim()) {
        // Answer pings without queueing behind a slow start/update.
        Ok(Request::Ping) => Response::Pong {
            version: SERVICE_VERSION.to_owned(),
        },
        Ok(Request::Logs { count }) => Response::Logs {
            lines: log.tail(count.min(500)),
        },
        Ok(request) => supervisor.lock().await.handle(request).await,
        Err(error) => Response::Error {
            message: format!("invalid request: {error}"),
        },
    };

    let mut reply = serde_json::to_string(&response)?;
    reply.push('\n');
    stream.get_mut().write_all(reply.as_bytes()).await?;
    stream.get_mut().flush().await?;
    Ok(())
}

#[cfg(windows)]
mod transport {
    use anyhow::Context as _;
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };

    /// SYSTEM and Administrators: full. Authenticated users: read/write, i.e.
    /// connect and talk — not change the pipe's security.
    const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)";

    pub async fn connect(path: &str) -> anyhow::Result<NamedPipeClient> {
        const ERROR_PIPE_BUSY: i32 = 231;
        for _ in 0..40 {
            match ClientOptions::new().open(path) {
                Ok(client) => return Ok(client),
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(error) => {
                    return Err(error).context(crate::i18n::t(
                        "增强模式服务未运行",
                        "The Enhanced Mode service isn't running",
                    ));
                }
            }
        }
        anyhow::bail!(crate::i18n::t(
            "增强模式服务繁忙",
            "The Enhanced Mode service is busy"
        ))
    }

    pub struct Listener {
        path: String,
        next: NamedPipeServer,
    }

    impl Listener {
        pub fn bind(path: &str) -> anyhow::Result<Self> {
            Ok(Self {
                path: path.to_owned(),
                next: create(path, true)?,
            })
        }

        pub async fn accept(&mut self) -> anyhow::Result<NamedPipeServer> {
            self.next.connect().await?;
            // Have the next instance ready before serving this one, so
            // concurrent clients never see "pipe not found".
            Ok(std::mem::replace(
                &mut self.next,
                create(&self.path, false)?,
            ))
        }
    }

    fn create(path: &str, first: bool) -> anyhow::Result<NamedPipeServer> {
        use windows::Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::{
                Authorization::{
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                },
                PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
            },
        };
        use windows::core::HSTRING;

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: valid SDDL string; the descriptor is freed with LocalFree
        // below, after CreateNamedPipe has copied it.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(PIPE_SDDL),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?;
            let mut attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let server = ServerOptions::new()
                // Refuse to start if someone else already squats the name.
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    path,
                    (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
                );
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            server.with_context(|| format!("failed to create service pipe {path}"))
        }
    }
}

#[cfg(unix)]
mod transport {
    use std::os::unix::fs::PermissionsExt as _;

    use anyhow::Context as _;
    use tokio::net::{UnixListener, UnixStream};

    pub async fn connect(path: &str) -> anyhow::Result<UnixStream> {
        UnixStream::connect(path).await.context(crate::i18n::t(
            "增强模式服务未运行",
            "The Enhanced Mode service isn't running",
        ))
    }

    pub struct Listener(UnixListener);

    impl Listener {
        /// Socket is 0666 so the desktop user can reach the root service;
        /// its directory is 0755 and root-owned.
        pub fn bind(path: &str) -> anyhow::Result<Self> {
            if let Some(dir) = std::path::Path::new(path).parent() {
                std::fs::create_dir_all(dir)?;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;
            }
            let _ = std::fs::remove_file(path);
            let listener = UnixListener::bind(path)
                .with_context(|| format!("failed to bind service socket {path}"))?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
            Ok(Self(listener))
        }

        pub async fn accept(&mut self) -> anyhow::Result<UnixStream> {
            Ok(self.0.accept().await?.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use super::{Request, Response, sanitize_config};

    fn sanitized(yaml: &str) -> anyhow::Result<Value> {
        let mut value: Value = serde_yaml::from_str(yaml).unwrap();
        sanitize_config(&mut value).map(|()| value)
    }

    #[test]
    fn strips_controller_and_ui_and_pins_provider_paths() {
        let value = sanitized(
            r#"
external-controller: 0.0.0.0:9090
external-ui: C:\evil
secret: x
rule-providers:
  ads: {type: http, url: "https://x", path: 'C:\Windows\System32\drivers\etc\hosts'}
proxy-providers:
  sub: {type: http, url: "https://y"}
"#,
        )
        .unwrap();
        assert!(value.get("external-controller").is_none());
        assert!(value.get("external-ui").is_none());
        assert!(value.get("secret").is_none());
        assert_eq!(
            value["rule-providers"]["ads"]["path"],
            Value::from("./providers/rule-providers-0.yaml")
        );
        assert_eq!(
            value["proxy-providers"]["sub"]["path"],
            Value::from("./providers/proxy-providers-0.yaml")
        );
    }

    #[test]
    fn rejects_file_providers_and_outside_paths() {
        assert!(sanitized("rule-providers:\n  a: {type: file, path: x.yaml}\n").is_err());
        assert!(sanitized("mmdb: 'C:\\secret.mmdb'\n").is_err());
        assert!(sanitized("mmdb: ../../x.mmdb\n").is_err());
        assert!(sanitized("listeners:\n  - {certificate: '\\\\server\\share\\k.pem'}\n").is_err());
    }

    #[test]
    fn allows_plain_names_and_base64_keys() {
        assert!(sanitized("mmdb: Country.mmdb\n").is_ok());
        assert!(sanitized("proxies:\n  - {name: v, type: vmess, ws-opts: {path: /ws}}\n").is_ok());
        assert!(
            sanitized("proxies:\n  - {name: wg, type: wireguard, private-key: '/aB+c/dEf==', ip: 10.0.0.2}\n")
                .is_ok()
        );
    }

    #[test]
    fn protocol_is_tagged_json() {
        let request = serde_json::to_string(&Request::Start {
            config: "a: 1".into(),
        })
        .unwrap();
        assert_eq!(request, r#"{"cmd":"start","config":"a: 1"}"#);
        let response: Response = serde_json::from_str(r#"{"reply":"pong","version":"1"}"#).unwrap();
        assert!(matches!(response, Response::Pong { .. }));
    }
}
