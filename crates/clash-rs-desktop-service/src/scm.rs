use std::{ffi::OsString, time::Duration};

use clash_rs_desktop_core::service::{SERVICE_NAME, SERVICE_PIPE, pipe_transport};
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

const SERVICE_DISPLAY_NAME: &str = "clash-rs Desktop Service";
const SERVICE_DESCRIPTION: &str =
    "Privileged clash-rs Desktop supervisor for TUN and elevated core lifecycle.";

define_windows_service!(ffi_service_main, service_main);

pub(crate) fn run_service_dispatcher() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

pub(crate) fn install_service() -> anyhow::Result<()> {
    // CREATE_SERVICE alone can't reopen an existing service, and CONNECT alone
    // can't create one — request both so a fresh install and a reconcile of an
    // already-installed service both work from the same handle.
    let manager =
        open_manager(ServiceManagerAccess::CREATE_SERVICE | ServiceManagerAccess::CONNECT)?;
    let service_binary_path = std::env::current_exe()?;
    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    match manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG) {
        Ok(service) => {
            service.set_description(SERVICE_DESCRIPTION)?;
            Ok(())
        }
        // The service is already registered. Treat install as idempotent:
        // reopen it and refresh its config so a moved/renamed binary (a fresh
        // unzip to a new folder) is corrected rather than failing the install.
        Err(error) if is_service_exists_error(&error) => {
            let service = manager.open_service(
                SERVICE_NAME,
                ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_CONFIG,
            )?;
            service.change_config(&service_info)?;
            service.set_description(SERVICE_DESCRIPTION)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// `true` when a `windows-service` error is the SCM "service already exists"
/// failure (`ERROR_SERVICE_EXISTS`, 1073) raised by `CreateServiceW`.
fn is_service_exists_error(error: &windows_service::Error) -> bool {
    const ERROR_SERVICE_EXISTS: i32 = 1073;
    matches!(
        error,
        windows_service::Error::Winapi(io_error)
            if io_error.raw_os_error() == Some(ERROR_SERVICE_EXISTS)
    )
}

pub(crate) fn uninstall_service() -> anyhow::Result<()> {
    let manager = open_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    )?;

    let status = service.query_status()?;
    if status.current_state != ServiceState::Stopped {
        let _ = service.stop();
        wait_for_state(&service, ServiceState::Stopped, Duration::from_secs(10))?;
    }

    service.delete()?;
    Ok(())
}

pub(crate) fn start_service() -> anyhow::Result<()> {
    let manager = open_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::START,
    )?;
    let status = service.query_status()?;
    if status.current_state == ServiceState::Running {
        return Ok(());
    }
    service.start(&[] as &[OsString])?;
    wait_for_state(&service, ServiceState::Running, Duration::from_secs(10))
}

pub(crate) fn stop_service() -> anyhow::Result<()> {
    let manager = open_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
    )?;
    let status = service.query_status()?;
    if status.current_state == ServiceState::Stopped {
        return Ok(());
    }
    service.stop()?;
    wait_for_state(&service, ServiceState::Stopped, Duration::from_secs(10))
}

pub(crate) fn service_status_text() -> anyhow::Result<String> {
    let manager = open_manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)?;
    let status = service.query_status()?;
    Ok(format!("{:?}", status.current_state))
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        eprintln!("{SERVICE_NAME} failed: {error}");
    }
}

fn run_service() -> anyhow::Result<()> {
    let (shutdown_sender, shutdown_receiver) = tokio::sync::watch::channel(false);
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_sender.send(true);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(pipe_transport::run_pipe_server_until(
        SERVICE_PIPE,
        shutdown_receiver,
    ));

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    result
}

fn open_manager(access: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, access)?)
}

fn wait_for_state(
    service: &windows_service::service::Service,
    expected_state: ServiceState,
    timeout: Duration,
) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        let status = service.query_status()?;
        if status.current_state == expected_state {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    anyhow::bail!("timed out waiting for service state {expected_state:?}")
}

#[cfg(test)]
mod tests {
    use super::is_service_exists_error;

    #[test]
    fn service_exists_error_matches_1073() {
        // CreateServiceW raises ERROR_SERVICE_EXISTS (1073) when the service is
        // already registered — the case install must treat as idempotent.
        let error = windows_service::Error::Winapi(std::io::Error::from_raw_os_error(1073));
        assert!(is_service_exists_error(&error));
    }

    #[test]
    fn other_winapi_errors_do_not_match() {
        // Access denied (5) and "does not exist" (1060) are genuine failures
        // and must not be swallowed as an already-installed no-op.
        let denied = windows_service::Error::Winapi(std::io::Error::from_raw_os_error(5));
        let missing = windows_service::Error::Winapi(std::io::Error::from_raw_os_error(1060));
        assert!(!is_service_exists_error(&denied));
        assert!(!is_service_exists_error(&missing));
    }
}
