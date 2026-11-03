//! Windows Service Control Manager glue.

use std::{ffi::OsString, path::Path, time::Duration};

use clash_rs_desktop_core::service::{
    SERVICE_DISPLAY_NAME, SERVICE_NAME, SERVICE_PIPE, ServicePaths, Supervisor, serve,
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
        ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};

const DESCRIPTION: &str = "Runs the Choclite network core for Enhanced Mode (TUN).";
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERROR_SERVICE_EXISTS: i32 = 1073;

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

/// Foreground mode for debugging (run from an elevated terminal).
pub fn run_console() -> anyhow::Result<()> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = stop_tx.send(true);
        });
        serve(
            SERVICE_PIPE,
            Supervisor::new(ServicePaths::system()),
            stop_rx,
        )
        .await
    })
}

fn winapi_code(error: &windows_service::Error) -> Option<i32> {
    match error {
        windows_service::Error::Winapi(io) => io.raw_os_error(),
        _ => None,
    }
}

fn manager(access: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, access)?)
}

pub fn install(binary: &Path) -> anyhow::Result<()> {
    let manager = manager(ServiceManagerAccess::CREATE_SERVICE | ServiceManagerAccess::CONNECT)?;
    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: binary.to_owned(),
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };
    let access = ServiceAccess::CHANGE_CONFIG | ServiceAccess::START;
    let service = match manager.create_service(&info, access) {
        Ok(service) => service,
        Err(error) if winapi_code(&error) == Some(ERROR_SERVICE_EXISTS) => {
            let service = manager.open_service(SERVICE_NAME, access)?;
            service.change_config(&info)?;
            service
        }
        Err(error) => return Err(error.into()),
    };
    service.set_description(DESCRIPTION)?;
    // Come back by itself if it ever crashes.
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(2),
            };
            3
        ]),
    })?;
    Ok(())
}

pub fn start() -> anyhow::Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::START,
    )?;
    if service.query_status()?.current_state != ServiceState::Running {
        service.start(&[] as &[OsString])?;
        wait_for(&service, ServiceState::Running)?;
    }
    Ok(())
}

pub fn stop_if_installed() -> anyhow::Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
    ) {
        Ok(service) => service,
        Err(error) if winapi_code(&error) == Some(ERROR_SERVICE_DOES_NOT_EXIST) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if service.query_status()?.current_state != ServiceState::Stopped {
        let _ = service.stop();
        wait_for(&service, ServiceState::Stopped)?;
    }
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    stop_if_installed()?;
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    match manager.open_service(SERVICE_NAME, ServiceAccess::DELETE) {
        Ok(service) => Ok(service.delete()?),
        Err(error) if winapi_code(&error) == Some(ERROR_SERVICE_DOES_NOT_EXIST) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn wait_for(
    service: &windows_service::service::Service,
    state: ServiceState,
) -> anyhow::Result<()> {
    for _ in 0..80 {
        if service.query_status()?.current_state == state {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    anyhow::bail!("timed out waiting for service to reach {state:?}")
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(error) = run_service() {
        crate::install::log(&format!("service failed: {error:#}"));
    }
}

fn run_service() -> anyhow::Result<()> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let handle = service_control_handler::register(SERVICE_NAME, move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop_tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;

    let status = |state, accept| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    handle.set_service_status(status(
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
    ))?;

    let result = tokio::runtime::Runtime::new()?.block_on(serve(
        SERVICE_PIPE,
        Supervisor::new(ServicePaths::system()),
        stop_rx,
    ));
    if let Err(error) = &result {
        crate::install::log(&format!("pipe server stopped: {error:#}"));
    }

    handle.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty()))?;
    result
}
