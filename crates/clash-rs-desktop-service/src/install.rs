//! Elevated install/uninstall. Everything the service will later run as
//! LocalSystem is placed in admin-only directories here, while we still have
//! the user's explicit (UAC) consent.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context as _;
use clash_rs_desktop_core::service::ServicePaths;
use sha2::{Digest, Sha256};

use crate::scm;

/// wintun 0.14.1 (WireGuard LLC), pinned by hash.
const WINTUN_URL: &str = "https://www.wintun.net/builds/wintun-0.14.1.zip";
const WINTUN_ZIP_SHA256: &str = "07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51";

fn wintun_dll() -> (&'static str, &'static str) {
    if cfg!(target_arch = "aarch64") {
        (
            "wintun/bin/arm64/wintun.dll",
            "f7ba89005544be9d85231a9e0d5f23b2d15b3311667e2dad0debd344918a3f80",
        )
    } else if cfg!(target_arch = "x86") {
        (
            "wintun/bin/x86/wintun.dll",
            "d694fa46ab4cfebcb2632d094c7aa97278eef2f8052438621766d863ae98a931",
        )
    } else {
        (
            "wintun/bin/amd64/wintun.dll",
            "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce",
        )
    }
}

pub fn log(line: &str) {
    let dir = ServicePaths::system().data_dir;
    let _ = fs::create_dir_all(&dir);
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("service.log"))
    {
        let _ = writeln!(file, "{line}");
    }
}

pub fn install(core: Option<PathBuf>) -> anyhow::Result<()> {
    let paths = ServicePaths::system();
    log(&format!("install from {:?}", std::env::current_exe()));

    // Replacing a running service exe fails; stop first (no-op if absent).
    scm::stop_if_installed()?;

    fs::create_dir_all(&paths.install_dir)?;
    fs::create_dir_all(&paths.data_dir)?;
    lock_down(&paths.install_dir)?;
    lock_down(&paths.data_dir)?;

    let me = std::env::current_exe()?;
    if !same_file(&me, &paths.service_binary()) {
        fs::copy(&me, paths.service_binary()).context("failed to copy service binary")?;
    }

    match core {
        Some(core) => {
            fs::copy(&core, paths.core_binary())
                .with_context(|| format!("failed to copy core from {}", core.display()))?;
        }
        None if !paths.core_binary().exists() => download_core(&paths)?,
        None => {}
    }

    ensure_wintun(&paths.install_dir)?;
    scm::install(&paths.service_binary())?;
    scm::start()?;
    log("installed and started");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let paths = ServicePaths::system();
    scm::uninstall()?;
    // The exe running this may live in install_dir; remove what we can.
    let _ = fs::remove_dir_all(&paths.data_dir);
    if let Ok(entries) = fs::read_dir(&paths.install_dir) {
        for entry in entries.flatten() {
            // The exe running this uninstall can't delete itself; have
            // Windows remove it at the next boot instead of leaving it.
            if fs::remove_file(entry.path()).is_err() {
                delete_on_reboot(&entry.path());
            }
        }
    }
    for dir in [
        paths.install_dir.clone(),
        paths
            .install_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        paths
            .data_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    ] {
        // Directories go only once empty; queued after the files above.
        if fs::remove_dir(&dir).is_err() && dir.exists() {
            delete_on_reboot(&dir);
        }
    }
    Ok(())
}

fn delete_on_reboot(path: &Path) {
    use windows::{
        Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW},
        core::HSTRING,
    };
    // SAFETY: plain Win32 call; needs admin, which the uninstaller has.
    unsafe {
        let _ = MoveFileExW(
            &HSTRING::from(path.as_os_str()),
            None,
            MOVEFILE_DELAY_UNTIL_REBOOT,
        );
    }
}

/// SYSTEM and Administrators: full. Users: read & execute. No inheritance,
/// so a user can't drop files (e.g. a DLL next to the core) in here.
fn lock_down(dir: &Path) -> anyhow::Result<()> {
    let status = Command::new("icacls")
        .arg(dir)
        .args([
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-18:(OI)(CI)F",
            "*S-1-5-32-544:(OI)(CI)F",
            "*S-1-5-32-545:(OI)(CI)RX",
            "/Q",
        ])
        .status()
        .context("failed to run icacls")?;
    anyhow::ensure!(status.success(), "icacls failed on {}", dir.display());
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn ensure_wintun(dir: &Path) -> anyhow::Result<()> {
    let (entry, dll_sha) = wintun_dll();
    let target = dir.join("wintun.dll");
    if fs::read(&target).is_ok_and(|bytes| sha256_hex(&bytes) == dll_sha) {
        return Ok(());
    }

    let zip = runtime()?
        .block_on(async {
            reqwest::get(WINTUN_URL)
                .await?
                .error_for_status()?
                .bytes()
                .await
        })
        .context("failed to download wintun")?;
    anyhow::ensure!(
        sha256_hex(&zip) == WINTUN_ZIP_SHA256,
        "wintun download hash mismatch"
    );

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))?;
    let mut dll = Vec::new();
    archive.by_name(entry)?.read_to_end(&mut dll)?;
    anyhow::ensure!(sha256_hex(&dll) == dll_sha, "wintun.dll hash mismatch");
    fs::write(&target, dll)?;
    Ok(())
}

fn download_core(paths: &ServicePaths) -> anyhow::Result<()> {
    use clash_rs_desktop_core::{CoreUpdateManager, CoreUpdateSettings, ManagedCoreFiles};
    runtime()?.block_on(async {
        let manager = CoreUpdateManager::new();
        let check = manager
            .check_latest(&CoreUpdateSettings::default(), None)
            .await?;
        let files = ManagedCoreFiles {
            current_binary: paths.core_binary(),
            rollback_binary: paths.install_dir.join("clash-rs.rollback.exe"),
            wintun_dll: paths.install_dir.join("wintun.dll"),
            active_metadata: paths.install_dir.join("core.json"),
        };
        manager
            .download_and_activate(&check.candidate, &files)
            .await?;
        anyhow::Ok(())
    })
}

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}
