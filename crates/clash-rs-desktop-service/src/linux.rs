//! Linux: the service is a systemd unit running as root; the app reaches it
//! over a unix socket. Installed once via `pkexec` (polkit password prompt).

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context as _;
use clash_rs_desktop_core::service::{SERVICE_PIPE, ServicePaths, Supervisor, serve};

const UNIT_NAME: &str = "choclite.service";
const UNIT_PATH: &str = "/etc/systemd/system/choclite.service";

/// `--service` (systemd) and `--console` (foreground debugging).
pub fn run() -> anyhow::Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            let _ = stop_tx.send(true);
        });
        let result = serve(
            SERVICE_PIPE,
            Supervisor::new(ServicePaths::system()),
            stop_rx,
        )
        .await;
        let _ = fs::remove_file(SERVICE_PIPE);
        result
    })
}

pub fn install(core: Option<PathBuf>) -> anyhow::Result<()> {
    // SAFETY: plain libc query.
    anyhow::ensure!(
        unsafe { libc::geteuid() } == 0,
        "administrator (root) privileges required"
    );
    anyhow::ensure!(
        Path::new("/run/systemd/system").exists(),
        "systemd not found; can't install the Enhanced Mode service"
    );
    let paths = ServicePaths::system();
    let _ = systemctl(&["stop", UNIT_NAME]);

    fs::create_dir_all(&paths.install_dir)?;
    fs::set_permissions(&paths.install_dir, fs::Permissions::from_mode(0o755))?;
    fs::create_dir_all(&paths.data_dir)?;
    fs::set_permissions(&paths.data_dir, fs::Permissions::from_mode(0o700))?;

    let me = std::env::current_exe()?;
    if fs::canonicalize(&me).ok() != fs::canonicalize(paths.service_binary()).ok() {
        copy_executable(&me, &paths.service_binary())?;
    }
    match core {
        Some(core) => copy_executable(&core, &paths.core_binary())?,
        None if !paths.core_binary().exists() => download_core(&paths)?,
        None => {}
    }

    fs::write(
        UNIT_PATH,
        format!(
            "[Unit]\n\
             Description=Choclite Service (TUN mode)\n\
             After=network-online.target\n\
             Wants=network-online.target\n\n\
             [Service]\n\
             ExecStart={} --service\n\
             Restart=on-failure\n\
             RestartSec=2\n\n\
             [Install]\n\
             WantedBy=multi-user.target\n",
            paths.service_binary().display()
        ),
    )?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", UNIT_NAME])?;
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let _ = systemctl(&["disable", "--now", UNIT_NAME]);
    let _ = fs::remove_file(UNIT_PATH);
    let _ = systemctl(&["daemon-reload"]);
    let paths = ServicePaths::system();
    let _ = fs::remove_dir_all(&paths.data_dir);
    let _ = fs::remove_dir_all(&paths.install_dir);
    Ok(())
}

/// Write via a temp file + rename: works even if the target is running
/// ("text file busy" only applies to writing in place).
fn copy_executable(from: &Path, to: &Path) -> anyhow::Result<()> {
    let staging = to.with_extension("new");
    fs::copy(from, &staging).with_context(|| format!("failed to copy {}", from.display()))?;
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))?;
    fs::rename(&staging, to)?;
    Ok(())
}

fn systemctl(args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("systemctl").args(args).status()?;
    anyhow::ensure!(status.success(), "systemctl {} failed", args.join(" "));
    Ok(())
}

fn download_core(paths: &ServicePaths) -> anyhow::Result<()> {
    use clash_rs_desktop_core::{CoreUpdateManager, CoreUpdateSettings, ManagedCoreFiles};
    tokio::runtime::Runtime::new()?.block_on(async {
        let manager = CoreUpdateManager::new();
        let check = manager
            .check_latest(&CoreUpdateSettings::default(), None)
            .await?;
        let files = ManagedCoreFiles {
            current_binary: paths.core_binary(),
            rollback_binary: paths.install_dir.join("clash-rs.rollback"),
            wintun_dll: paths.install_dir.join("unused"),
            active_metadata: paths.install_dir.join("core.json"),
        };
        manager
            .download_and_activate(&check.candidate, &files)
            .await?;
        anyhow::Ok(())
    })
}

pub fn log(line: &str) {
    // systemd captures stderr into the journal.
    eprintln!("{line}");
}
