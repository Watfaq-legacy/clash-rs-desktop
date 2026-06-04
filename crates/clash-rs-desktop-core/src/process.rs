use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElevationReason {
    TunEnabled,
    RouteOrDnsMutation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRequirements {
    pub requires_service: bool,
    pub reasons: Vec<ElevationReason>,
}

impl LaunchRequirements {
    pub fn normal() -> Self {
        Self {
            requires_service: false,
            reasons: Vec::new(),
        }
    }

    pub fn for_tun() -> Self {
        Self {
            requires_service: true,
            reasons: vec![ElevationReason::TunEnabled],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreOwner {
    UiProcess {
        pid: u32,
    },
    WindowsService {
        service_name: String,
        pid: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoreState {
    Stopped,
    Starting { elevated: bool },
    Running { elevated: bool, owner: CoreOwner },
    NeedsService { reasons: Vec<ElevationReason> },
    Stopping,
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreLaunchRequest {
    pub binary_path: PathBuf,
    pub runtime_dir: PathBuf,
    pub config_path: PathBuf,
    pub core_pipe_path: String,
}

impl CoreLaunchRequest {
    pub fn clash_args(&self) -> Vec<String> {
        vec![
            "-d".to_owned(),
            self.runtime_dir.display().to_string(),
            "-f".to_owned(),
            config_arg(&self.runtime_dir, &self.config_path),
            "--ext-ctl-pipe".to_owned(),
            self.core_pipe_path.clone(),
        ]
    }

    pub fn test_config_args(&self) -> Vec<String> {
        vec![
            "-d".to_owned(),
            self.runtime_dir.display().to_string(),
            "-f".to_owned(),
            config_arg(&self.runtime_dir, &self.config_path),
            "--test-config".to_owned(),
        ]
    }

    pub fn validate_config(&self) -> anyhow::Result<()> {
        let mut command = Command::new(&self.binary_path);
        command.args(self.test_config_args());
        if let Some(binary_dir) = self.binary_path.parent() {
            command.current_dir(binary_dir);
        }
        hide_console_window(&mut command);

        let output = command.output()?;
        if output.status.success() {
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "clash-rs config validation failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout.trim(),
            stderr.trim()
        )
    }
}

#[derive(Debug)]
pub struct ClashProcessManager {
    state: CoreState,
    child: Option<Child>,
}

impl Default for ClashProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClashProcessManager {
    pub fn new() -> Self {
        Self {
            state: CoreState::Stopped,
            child: None,
        }
    }

    pub fn state(&mut self) -> CoreState {
        self.refresh_state();
        self.state.clone()
    }

    pub fn start_direct(&mut self, request: &CoreLaunchRequest) -> anyhow::Result<CoreState> {
        self.refresh_state();
        if self.child.is_some() {
            anyhow::bail!("clash-rs core is already running");
        }

        self.state = CoreState::Starting { elevated: false };

        let mut command = Command::new(&request.binary_path);
        command.args(request.clash_args());
        if let Some(binary_dir) = request.binary_path.parent() {
            command.current_dir(binary_dir);
        }
        hide_console_window(&mut command);

        let child = command.spawn()?;
        let pid = child.id();
        self.child = Some(child);
        self.state = CoreState::Running {
            elevated: false,
            owner: CoreOwner::UiProcess { pid },
        };
        Ok(self.state.clone())
    }

    pub fn stop(&mut self) -> anyhow::Result<CoreState> {
        self.refresh_state();
        let Some(mut child) = self.child.take() else {
            self.state = CoreState::Stopped;
            return Ok(self.state.clone());
        };

        self.state = CoreState::Stopping;
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        child.wait()?;
        self.state = CoreState::Stopped;
        Ok(self.state.clone())
    }

    fn refresh_state(&mut self) {
        let Some(child) = &mut self.child else {
            if !matches!(
                self.state,
                CoreState::NeedsService { .. } | CoreState::Failed { .. }
            ) {
                self.state = CoreState::Stopped;
            }
            return;
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                self.state = if status.success() {
                    CoreState::Stopped
                } else {
                    CoreState::Failed {
                        message: format!("clash-rs exited with {status}"),
                    }
                };
            }
            Ok(None) => {}
            Err(error) => {
                self.child = None;
                self.state = CoreState::Failed {
                    message: format!("failed to query clash-rs process: {error}"),
                };
            }
        }
    }
}

impl Drop for ClashProcessManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

fn config_arg(runtime_dir: &Path, config_path: &Path) -> String {
    if let Ok(relative) = config_path.strip_prefix(runtime_dir)
        && !relative.as_os_str().is_empty()
    {
        return relative.display().to_string();
    }

    config_path.display().to_string()
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

pub fn write_core_pid(path: &Path, pid: u32) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, pid.to_string())?;
    Ok(())
}

pub fn read_core_pid(path: &Path) -> Option<u32> {
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok()
}

pub fn clear_core_pid(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(windows)]
pub fn pid_is_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        if handle.is_invalid() {
            return false;
        }
        let mut code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut code as *mut u32).is_ok()
            && code == STILL_ACTIVE.0 as u32;
        let _ = CloseHandle(handle);
        alive
    }
}

#[cfg(not(windows))]
pub fn pid_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
pub fn kill_pid(pid: u32) -> anyhow::Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|error| anyhow::anyhow!("OpenProcess({pid}) failed: {error}"))?;
        if handle.is_invalid() {
            anyhow::bail!("OpenProcess({pid}) returned invalid handle");
        }
        let result = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result.map_err(|error| anyhow::anyhow!("TerminateProcess({pid}) failed: {error}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn kill_pid(_pid: u32) -> anyhow::Result<()> {
    anyhow::bail!("kill_pid only implemented on Windows")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::CoreLaunchRequest;

    #[test]
    fn launch_request_uses_relative_config_inside_runtime_dir() {
        let request = CoreLaunchRequest {
            binary_path: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\core\clash-rs.exe"),
            runtime_dir: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\runtime"),
            config_path: PathBuf::from(r"C:\ProgramData\clash-rs-desktop\runtime\generated.yaml"),
            core_pipe_path: r"\\.\pipe\clash-rs-desktop-core".to_owned(),
        };

        assert_eq!(
            request.clash_args(),
            vec![
                "-d",
                r"C:\ProgramData\clash-rs-desktop\runtime",
                "-f",
                "generated.yaml",
                "--ext-ctl-pipe",
                r"\\.\pipe\clash-rs-desktop-core",
            ]
        );

        assert_eq!(
            request.test_config_args(),
            vec![
                "-d",
                r"C:\ProgramData\clash-rs-desktop\runtime",
                "-f",
                "generated.yaml",
                "--test-config",
            ]
        );
    }
}
