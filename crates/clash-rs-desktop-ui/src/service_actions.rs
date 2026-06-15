use clash_rs_desktop_core::{CoreState, ServiceCommand, ServiceResponse, pipe_transport};

pub(crate) fn describe_core_state(state: &CoreState) -> String {
    match state {
        CoreState::Stopped => "stopped".to_owned(),
        CoreState::Starting { elevated } => format!("starting (elevated: {elevated})"),
        CoreState::Running { elevated, owner } => {
            format!("running (elevated: {elevated}, owner: {owner:?})")
        }
        CoreState::NeedsService { .. } => "needs service".to_owned(),
        CoreState::Stopping => "stopping".to_owned(),
        CoreState::Failed { message } => format!("failed ({message})"),
    }
}

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
    run_service_exe_elevated("--install")
        .map(|output| format!("service install\n{output}"))
        .unwrap_or_else(|error| format!("service install failed: {error}"))
}

pub(crate) fn reactor_service_uninstall_text() -> String {
    run_service_exe_elevated("--uninstall")
        .map(|output| format!("service uninstall\n{output}"))
        .unwrap_or_else(|error| format!("service uninstall failed: {error}"))
}

pub(crate) fn reactor_service_start_text() -> String {
    run_service_exe_elevated("--start-service")
        .map(|output| format!("service start\n{output}"))
        .unwrap_or_else(|error| format!("service start failed: {error}"))
}

pub(crate) fn reactor_service_process_stop_text() -> String {
    run_service_exe_elevated("--stop-service")
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

/// Launch the service executable elevated via the UAC `runas` verb. SCM
/// service install/uninstall/start/stop require administrator rights that the
/// non-elevated UI process does not have. ShellExecute cannot capture stdout,
/// so success is reported from the elevated process exit code instead.
#[cfg(windows)]
fn run_service_exe_elevated(argument: &str) -> anyhow::Result<String> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::PCWSTR;

    let executable = service_executable_path()?;

    fn to_wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let verb = to_wide(std::ffi::OsStr::new("runas"));
    let file = to_wide(executable.as_os_str());
    let params = to_wide(std::ffi::OsStr::new(argument));

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|error| anyhow::anyhow!("elevation request failed or was declined: {error}"))?;
    }

    let process = info.hProcess;
    if process.is_invalid() {
        anyhow::bail!("elevated process did not start");
    }

    let exit_code = unsafe {
        if WaitForSingleObject(process, INFINITE) != WAIT_OBJECT_0 {
            let _ = CloseHandle(process);
            anyhow::bail!("failed to wait for elevated process");
        }
        let mut code: u32 = 0;
        let got = GetExitCodeProcess(process, &mut code as *mut u32);
        let _ = CloseHandle(process);
        got.map_err(|error| anyhow::anyhow!("failed to read elevated exit code: {error}"))?;
        code
    };

    if exit_code != 0 {
        anyhow::bail!("elevated command exited with code {exit_code}");
    }

    Ok(format!("{} {argument} completed", executable.display()))
}

#[cfg(not(windows))]
fn run_service_exe_elevated(_argument: &str) -> anyhow::Result<String> {
    anyhow::bail!("service elevation is only supported on Windows")
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
