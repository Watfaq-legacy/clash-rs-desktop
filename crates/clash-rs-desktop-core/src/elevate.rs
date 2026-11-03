//! Run a program with administrator rights (one UAC prompt) and wait for it.

use std::path::Path;

/// Outcome of an elevated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevated {
    Exited(u32),
    /// The user said "no" to the UAC prompt.
    Cancelled,
}

#[cfg(windows)]
pub fn run_elevated(program: &Path, args: &str) -> anyhow::Result<Elevated> {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, ERROR_CANCELLED},
            System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
            UI::{
                Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
                WindowsAndMessaging::SW_HIDE,
            },
        },
        core::{HSTRING, PCWSTR, w},
    };

    let file = HSTRING::from(program.as_os_str());
    let params = HSTRING::from(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    // SAFETY: `info` and the strings it points to outlive the call; the
    // returned process handle is waited on and closed below.
    unsafe {
        if let Err(error) = ShellExecuteExW(&mut info) {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                return Ok(Elevated::Cancelled);
            }
            return Err(error.into());
        }
        let process = info.hProcess;
        anyhow::ensure!(!process.is_invalid(), "elevated process did not start");
        WaitForSingleObject(process, INFINITE);
        let mut code = 0u32;
        let result = GetExitCodeProcess(process, &mut code);
        let _ = CloseHandle(process);
        result?;
        Ok(Elevated::Exited(code))
    }
}

/// Linux: `pkexec` shows the desktop's polkit password dialog.
#[cfg(not(windows))]
pub fn run_elevated(program: &Path, args: &str) -> anyhow::Result<Elevated> {
    let status = std::process::Command::new("pkexec")
        .arg(program)
        .args(args.split_whitespace())
        .status()?;
    // pkexec: 126 = dialog dismissed / not authorised.
    Ok(match status.code() {
        Some(126) | Some(127) => Elevated::Cancelled,
        Some(code) => Elevated::Exited(code as u32),
        None => Elevated::Cancelled,
    })
}
