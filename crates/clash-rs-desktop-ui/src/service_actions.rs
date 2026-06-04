use clash_rs_desktop_core::{ServiceCommand, ServiceResponse, pipe_transport};

use crate::runtime_actions::describe_core_state;

pub(crate) fn reactor_service_status_text() -> String {
    let scm_status = run_service_exe_command("--service-status")
        .unwrap_or_else(|error| format!("SCM status unavailable: {error}"));

    match send_service_command_sync(ServiceCommand::Status) {
        Ok(response) => format!(
            "service status\nSCM: {scm_status}\n{}",
            describe_service_response(&response)
        ),
        Err(error) => format!("service status\nSCM: {scm_status}\nservice pipe failed: {error}"),
    }
}

pub(crate) fn reactor_service_install_text() -> String {
    run_service_exe_command("--install")
        .map(|output| format!("service install\n{output}"))
        .unwrap_or_else(|error| format!("service install failed: {error}"))
}

pub(crate) fn reactor_service_uninstall_text() -> String {
    run_service_exe_command("--uninstall")
        .map(|output| format!("service uninstall\n{output}"))
        .unwrap_or_else(|error| format!("service uninstall failed: {error}"))
}

pub(crate) fn reactor_service_start_text() -> String {
    run_service_exe_command("--start-service")
        .map(|output| format!("service start\n{output}"))
        .unwrap_or_else(|error| format!("service start failed: {error}"))
}

pub(crate) fn reactor_service_process_stop_text() -> String {
    run_service_exe_command("--stop-service")
        .map(|output| format!("service process stop\n{output}"))
        .unwrap_or_else(|error| format!("service process stop failed: {error}"))
}

pub(crate) fn reactor_service_stop_text() -> String {
    match send_service_command_sync(ServiceCommand::StopCore) {
        Ok(response) => format!("service stop\n{}", describe_service_response(&response)),
        Err(error) => format!("service stop failed: {error}"),
    }
}

fn run_service_exe_command(argument: &str) -> anyhow::Result<String> {
    let executable = service_executable_path()?;
    let output = std::process::Command::new(&executable)
        .arg(argument)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        anyhow::bail!(
            "{} exited with {}{}{}",
            executable.display(),
            output.status,
            if stderr.is_empty() { "" } else { ": " },
            stderr
        );
    }

    Ok(if stdout.is_empty() { stderr } else { stdout })
}

fn service_executable_path() -> anyhow::Result<std::path::PathBuf> {
    let current_exe = std::env::current_exe()?;
    let service_exe = current_exe.with_file_name("clash-rs-desktop-service.exe");
    if service_exe.exists() {
        return Ok(service_exe);
    }

    anyhow::bail!(
        "service executable not found next to UI ({})",
        service_exe.display()
    )
}

pub(crate) fn send_service_command_sync(
    command: ServiceCommand,
) -> anyhow::Result<ServiceResponse> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let client =
            pipe_transport::ServicePipeClient::new(clash_rs_desktop_core::service::SERVICE_PIPE);
        client.send(&command).await
    })
}

pub(crate) fn describe_service_response(response: &ServiceResponse) -> String {
    match response {
        ServiceResponse::Ok => "service: ok".to_owned(),
        ServiceResponse::State { state } => format!("service: {}", describe_core_state(state)),
        ServiceResponse::Error { message } => format!("service error: {message}"),
    }
}
