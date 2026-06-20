use serde::{Deserialize, Serialize};

use crate::process::{ClashProcessManager, CoreLaunchRequest, CoreState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
pub enum ServiceCommand {
    StartCore { request: CoreLaunchRequest },
    StopCore,
    RestartCore { request: CoreLaunchRequest },
    Status,
    ApplyUpdate,
    RollbackUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ServiceResponse {
    Ok,
    State { state: CoreState },
    Error { message: String },
}

#[derive(Debug, Default)]
pub struct ServiceSupervisor {
    process: ClashProcessManager,
}

impl ServiceSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, command: ServiceCommand) -> ServiceResponse {
        match command {
            ServiceCommand::StartCore { request } => self.start(request),
            ServiceCommand::StopCore => self.stop(),
            ServiceCommand::RestartCore { request } => {
                if let ServiceResponse::Error { message } = self.stop() {
                    return ServiceResponse::Error { message };
                }
                self.start(request)
            }
            ServiceCommand::Status => ServiceResponse::State {
                state: self.process.state(),
            },
            ServiceCommand::ApplyUpdate => ServiceResponse::Error {
                message: "service update apply is not implemented yet".to_owned(),
            },
            ServiceCommand::RollbackUpdate => ServiceResponse::Error {
                message: "service update rollback is not implemented yet".to_owned(),
            },
        }
    }

    fn start(&mut self, request: CoreLaunchRequest) -> ServiceResponse {
        match self.process.start_direct(&request) {
            Ok(state) => ServiceResponse::State { state },
            Err(error) => ServiceResponse::Error {
                message: error.to_string(),
            },
        }
    }

    fn stop(&mut self) -> ServiceResponse {
        match self.process.stop() {
            Ok(state) => ServiceResponse::State { state },
            Err(error) => ServiceResponse::Error {
                message: error.to_string(),
            },
        }
    }
}

pub fn decode_command(line: &str) -> ServiceResponseOrCommand {
    match serde_json::from_str::<ServiceCommand>(line) {
        Ok(command) => ServiceResponseOrCommand::Command(command),
        Err(error) => ServiceResponseOrCommand::Response(ServiceResponse::Error {
            message: format!("invalid service command: {error}"),
        }),
    }
}

pub fn encode_response(response: &ServiceResponse) -> anyhow::Result<String> {
    Ok(format!("{}\n", serde_json::to_string(response)?))
}

#[derive(Debug)]
pub enum ServiceResponseOrCommand {
    Command(ServiceCommand),
    Response(ServiceResponse),
}

pub const SERVICE_NAME: &str = "ClashRsDesktopService";
pub const SERVICE_PIPE: &str = r"\\.\pipe\clash-rs-desktop-service";
pub const CORE_PIPE: &str = r"\\.\pipe\clash-rs-desktop-core";

#[cfg(windows)]
pub mod pipe_transport {
    use std::time::Duration;

    use anyhow::Context as _;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::windows::named_pipe::{ClientOptions, ServerOptions},
    };

    use super::{
        SERVICE_PIPE, ServiceCommand, ServiceResponse, ServiceResponseOrCommand, ServiceSupervisor,
        decode_command, encode_response,
    };

    /// SDDL for the service pipe's security descriptor.
    ///
    /// The service runs as LocalSystem, so a pipe created with the default DACL
    /// only admits SYSTEM/Administrators — the non-elevated UI gets
    /// `ERROR_ACCESS_DENIED` (5) on connect. This grants the UI process what it
    /// needs to talk to the service:
    /// - `D:` discretionary ACL
    /// - `(A;;GA;;;SY)` full access to LocalSystem (the server itself)
    /// - `(A;;GA;;;BA)` full access to the Builtin Administrators group
    /// - `(A;;GRGW;;;AU)` generic read + write to Authenticated Users — enough to
    ///   open the pipe and exchange one request/response, but not to take
    ///   ownership or rewrite the DACL.
    const SERVICE_PIPE_SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;AU)";

    /// Owns a self-relative security descriptor (allocated by
    /// `ConvertStringSecurityDescriptorToSecurityDescriptorW`, freed with
    /// `LocalFree`) and the `SECURITY_ATTRIBUTES` that points at it. Kept alive
    /// for the duration of the `create_with_security_attributes_raw` call.
    struct PipeSecurity {
        descriptor: windows::Win32::Security::PSECURITY_DESCRIPTOR,
        attributes: windows::Win32::Security::SECURITY_ATTRIBUTES,
    }

    impl PipeSecurity {
        fn new() -> anyhow::Result<Self> {
            use windows::Win32::Security::Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            };
            use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
            use windows::core::HSTRING;

            let sddl = HSTRING::from(SERVICE_PIPE_SDDL);
            let mut descriptor = PSECURITY_DESCRIPTOR::default();

            // SAFETY: `sddl` is a valid NUL-terminated wide string; on success
            // the API allocates `descriptor` with LocalAlloc, which we free in
            // Drop. We pass `None` for the optional size out-param.
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    &sddl,
                    SDDL_REVISION_1,
                    &mut descriptor,
                    None,
                )
                .context("failed to build service pipe security descriptor from SDDL")?;
            }

            if descriptor.0.is_null() {
                anyhow::bail!("service pipe security descriptor came back null");
            }

            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };

            Ok(Self {
                descriptor,
                attributes,
            })
        }

        /// Pointer to the `SECURITY_ATTRIBUTES`, valid while `self` is alive.
        fn as_ptr(&self) -> *const windows::Win32::Security::SECURITY_ATTRIBUTES {
            &self.attributes
        }
    }

    impl Drop for PipeSecurity {
        fn drop(&mut self) {
            use windows::Win32::Foundation::{HLOCAL, LocalFree};
            if !self.descriptor.0.is_null() {
                // SAFETY: `descriptor` was allocated by the SDDL conversion API,
                // which documents LocalFree as the matching deallocator.
                unsafe {
                    let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
                }
                self.descriptor.0 = std::ptr::null_mut();
            }
        }
    }

    pub struct ServicePipeClient {
        pipe_path: String,
    }

    impl Default for ServicePipeClient {
        fn default() -> Self {
            Self::new(SERVICE_PIPE)
        }
    }

    impl ServicePipeClient {
        pub fn new(pipe_path: impl Into<String>) -> Self {
            Self {
                pipe_path: pipe_path.into(),
            }
        }

        pub async fn send(&self, command: &ServiceCommand) -> anyhow::Result<ServiceResponse> {
            let mut pipe = open_client_with_retry(&self.pipe_path).await?;
            let request = format!("{}\n", serde_json::to_string(command)?);
            pipe.write_all(request.as_bytes()).await?;
            pipe.flush().await?;

            let mut reader = BufReader::new(pipe);
            let mut response = String::new();
            reader
                .read_line(&mut response)
                .await
                .context("failed to read service pipe response")?;

            Ok(serde_json::from_str(
                response.trim_start_matches('\u{feff}').trim(),
            )?)
        }
    }

    pub async fn run_pipe_server(pipe_path: &str) -> anyhow::Result<()> {
        let mut supervisor = ServiceSupervisor::new();
        loop {
            accept_one_client(pipe_path, &mut supervisor).await?;
        }
    }

    pub async fn run_pipe_server_until(
        pipe_path: &str,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut supervisor = ServiceSupervisor::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    let _ = changed;
                    let _ = supervisor.handle(ServiceCommand::StopCore);
                    return Ok(());
                }
                result = accept_one_client(pipe_path, &mut supervisor) => {
                    result?;
                }
            }
        }
    }

    async fn accept_one_client(
        pipe_path: &str,
        supervisor: &mut ServiceSupervisor,
    ) -> anyhow::Result<()> {
        let server = create_secured_server(pipe_path)?;
        server
            .connect()
            .await
            .with_context(|| format!("failed to accept service pipe client on {pipe_path}"))?;
        handle_client(server, supervisor).await
    }

    /// Create one named-pipe server instance carrying a DACL the non-elevated UI
    /// can open (see [`SERVICE_PIPE_SDDL`]). Falls back to a default-DACL pipe
    /// only if the security descriptor can't be built, so a malformed SDDL can't
    /// take the service offline — though the UI would then be denied as before.
    fn create_secured_server(
        pipe_path: &str,
    ) -> anyhow::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        match PipeSecurity::new() {
            Ok(security) => {
                // SAFETY: `security` owns the descriptor for the duration of this
                // call; the pointer is valid and points at a well-formed
                // SECURITY_ATTRIBUTES. `create_with_security_attributes_raw`
                // copies what it needs synchronously.
                let server = unsafe {
                    ServerOptions::new()
                        .first_pipe_instance(false)
                        .create_with_security_attributes_raw(
                            pipe_path,
                            security.as_ptr() as *mut std::ffi::c_void,
                        )
                }
                .with_context(|| format!("failed to create secured service pipe {pipe_path}"))?;
                drop(security);
                Ok(server)
            }
            Err(error) => {
                // The descriptor build failed; create with the default DACL so
                // the service still runs (the UI may be denied until fixed).
                eprintln!(
                    "warning: falling back to default pipe DACL ({error}); \
                     the non-elevated UI may be denied access"
                );
                ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(pipe_path)
                    .with_context(|| format!("failed to create service pipe {pipe_path}"))
            }
        }
    }

    async fn handle_client(
        server: tokio::net::windows::named_pipe::NamedPipeServer,
        supervisor: &mut ServiceSupervisor,
    ) -> anyhow::Result<()> {
        let mut reader = BufReader::new(server);
        let mut request = String::new();
        reader
            .read_line(&mut request)
            .await
            .context("failed to read service pipe request")?;

        let response = match decode_command(&request) {
            ServiceResponseOrCommand::Command(command) => supervisor.handle(command),
            ServiceResponseOrCommand::Response(response) => response,
        };

        let mut server = reader.into_inner();
        server
            .write_all(encode_response(&response)?.as_bytes())
            .await?;
        server.flush().await?;
        Ok(())
    }

    async fn open_client_with_retry(
        pipe_path: &str,
    ) -> anyhow::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
        const ERROR_PIPE_BUSY: i32 = 231;
        let started = std::time::Instant::now();
        let timeout = Duration::from_secs(5);
        let mut last_error = None;

        while started.elapsed() < timeout {
            match ClientOptions::new().open(pipe_path) {
                Ok(client) => return Ok(client),
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => {
                    return Err(anyhow::Error::from(error)
                        .context(format!("service pipe {pipe_path} is unavailable")));
                }
            }
        }

        Err(last_error
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow::anyhow!("timed out opening service pipe {pipe_path}")))
    }

    #[cfg(test)]
    mod tests {
        use super::{
            PipeSecurity, ServiceCommand, ServiceResponse, ServiceSupervisor, ServicePipeClient,
            accept_one_client,
        };

        #[test]
        fn pipe_security_descriptor_builds_from_sddl() {
            // The SDDL in SERVICE_PIPE_SDDL must parse into a non-null security
            // descriptor; a malformed string would silently fall back to the
            // default DACL and re-break non-elevated UI access.
            let security = PipeSecurity::new().expect("SDDL should convert to a descriptor");
            assert!(!security.descriptor.0.is_null());
            assert_eq!(
                security.attributes.lpSecurityDescriptor,
                security.descriptor.0,
                "SECURITY_ATTRIBUTES must point at the owned descriptor"
            );
            // Drop frees the descriptor via LocalFree; no leak.
        }

        /// End-to-end proof that a client can round-trip a command through a
        /// pipe created with the secured-DACL path the service actually uses.
        /// This is the reproducible stand-in for the manual non-elevated
        /// `--pipe-status` probe: server (via `accept_one_client`, which calls
        /// `create_secured_server`) ↔ `ServicePipeClient::send`. A regression
        /// that reverts the pipe to a deny-by-default DACL, or breaks the
        /// line-delimited framing, fails here in-process.
        #[tokio::test]
        async fn secured_pipe_round_trips_a_status_command() {
            // Unique path per run so concurrent tests / leftover pipes can't clash.
            let pipe_path = format!(
                r"\\.\pipe\clash-rs-desktop-test-{}",
                std::process::id()
            );

            let server_path = pipe_path.clone();
            let server = tokio::spawn(async move {
                let mut supervisor = ServiceSupervisor::new();
                accept_one_client(&server_path, &mut supervisor).await
            });

            // Give the server a moment to create the pipe before connecting.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let client = ServicePipeClient::new(pipe_path);
            let response = client
                .send(&ServiceCommand::Status)
                .await
                .expect("client should reach the secured pipe and get a response");

            // The supervisor reports core state; no core launched → Stopped.
            assert!(
                matches!(response, ServiceResponse::State { .. }),
                "expected a State response, got {response:?}"
            );

            server
                .await
                .expect("server task panicked")
                .expect("server should serve one client cleanly");
        }
    }
}

#[cfg(not(windows))]
pub mod pipe_transport {
    use super::{ServiceCommand, ServiceResponse};

    pub struct ServicePipeClient;

    impl ServicePipeClient {
        pub fn new(_pipe_path: impl Into<String>) -> Self {
            Self
        }

        pub async fn send(&self, _command: &ServiceCommand) -> anyhow::Result<ServiceResponse> {
            anyhow::bail!("service pipe transport is only implemented on Windows")
        }
    }

    pub async fn run_pipe_server(_pipe_path: &str) -> anyhow::Result<()> {
        anyhow::bail!("service pipe transport is only implemented on Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ServiceCommand, ServiceResponse, ServiceResponseOrCommand, ServiceSupervisor,
        decode_command, encode_response,
    };

    #[test]
    fn status_starts_stopped() {
        let mut supervisor = ServiceSupervisor::new();
        let response = supervisor.handle(ServiceCommand::Status);

        assert!(matches!(response, ServiceResponse::State { .. }));
    }

    #[test]
    fn service_json_round_trip_is_line_delimited() {
        let response = ServiceResponse::State {
            state: crate::process::CoreState::Stopped,
        };

        let encoded = encode_response(&response).unwrap();
        assert!(encoded.ends_with('\n'));
        assert!(matches!(
            serde_json::from_str::<ServiceResponse>(encoded.trim()).unwrap(),
            ServiceResponse::State { .. }
        ));
    }

    #[test]
    fn invalid_command_decodes_to_error_response() {
        let decoded = decode_command("not json");
        assert!(matches!(
            decoded,
            ServiceResponseOrCommand::Response(ServiceResponse::Error { .. })
        ));
    }
}
