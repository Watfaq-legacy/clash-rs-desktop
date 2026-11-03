//! `clash-rs-desktop-service` — the privileged half of Choclite's TUN mode.
//!
//! ```text
//! --install [--core <path>]   (elevated) copy into a root/admin-only dir,
//!                             register and start the service
//! --uninstall                 (elevated) stop and remove the service
//! --service                   entry point used by the SCM / systemd
//! --console                   run the server in the foreground (debug)
//! ```
//! Windows: SCM service + named pipe. Linux: systemd unit + unix socket.

#[cfg(windows)]
mod install;
#[cfg(unix)]
mod linux;
#[cfg(windows)]
mod scm;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let core = args
        .iter()
        .position(|arg| arg == "--core")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);

    let result = if flag("--service") {
        run_service()
    } else if flag("--install") {
        install(core)
    } else if flag("--uninstall") {
        uninstall()
    } else if flag("--console") {
        run_console()
    } else {
        eprintln!("usage: --install [--core <path>] | --uninstall | --service | --console");
        return std::process::ExitCode::from(2);
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // Elevated runs have no visible console; keep a trace.
            log(&format!("error: {error:#}"));
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
use {install::install, install::log, install::uninstall, scm::run_console};

#[cfg(windows)]
fn run_service() -> anyhow::Result<()> {
    scm::run_dispatcher()
}

#[cfg(unix)]
use linux::{install, log, run as run_service, run as run_console, uninstall};
