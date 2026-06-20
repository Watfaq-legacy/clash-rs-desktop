use std::io::{self, BufRead};

use clash_rs_desktop_core::service::{
    CORE_PIPE, SERVICE_NAME, SERVICE_PIPE, ServiceCommand, ServiceResponse, ServiceSupervisor,
    pipe_transport,
};

#[cfg(windows)]
mod scm;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    eprintln!("{SERVICE_NAME}");
    eprintln!("service pipe: {SERVICE_PIPE}");
    eprintln!("core pipe: {CORE_PIPE}");

    if std::env::args().any(|arg| arg == "--pipe-status") {
        let client = pipe_transport::ServicePipeClient::new(SERVICE_PIPE);
        let response = client.send(&ServiceCommand::Status).await?;
        println!("{}", serde_json::to_string(&response)?);
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--install") {
        scm::install_service()?;
        println!("installed {SERVICE_NAME}");
        return Ok(());
    }

    // Install (idempotently) and immediately start, under a single elevation so
    // the UI's one-click "Install & start" reaches a Running service without a
    // second UAC prompt.
    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--install-start") {
        scm::install_service()?;
        scm::start_service()?;
        println!("installed and started {SERVICE_NAME}");
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--uninstall") {
        scm::uninstall_service()?;
        println!("uninstalled {SERVICE_NAME}");
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--start-service") {
        scm::start_service()?;
        println!("started {SERVICE_NAME}");
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--stop-service") {
        scm::stop_service()?;
        println!("stopped {SERVICE_NAME}");
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--service-status") {
        println!("{}", scm::service_status_text()?);
        return Ok(());
    }

    #[cfg(windows)]
    if std::env::args().any(|arg| arg == "--service") {
        return scm::run_service_dispatcher();
    }

    if std::env::args().any(|arg| arg == "--pipe") {
        eprintln!("serving line-delimited JSON commands on service pipe");
        return pipe_transport::run_pipe_server(SERVICE_PIPE).await;
    }

    eprintln!("reading line-delimited JSON service commands from stdin");

    run_stdin_host()
}

fn run_stdin_host() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut supervisor = ServiceSupervisor::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<ServiceCommand>(&line) {
            Ok(command) => supervisor.handle(command),
            Err(error) => ServiceResponse::Error {
                message: format!("invalid service command: {error}"),
            },
        };

        println!("{}", serde_json::to_string(&response)?);
    }

    Ok(())
}
