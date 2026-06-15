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
        let server = ServerOptions::new()
            .first_pipe_instance(false)
            .create(pipe_path)
            .with_context(|| format!("failed to create service pipe {pipe_path}"))?;
        server
            .connect()
            .await
            .with_context(|| format!("failed to accept service pipe client on {pipe_path}"))?;
        handle_client(server, supervisor).await
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
