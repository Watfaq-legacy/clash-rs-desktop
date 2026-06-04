use std::path::PathBuf;

use clash_rs_desktop_core::AppPaths;

pub(crate) fn reactor_paths() -> anyhow::Result<AppPaths> {
    resolve_app_paths()
}

pub(crate) fn resolve_app_paths() -> anyhow::Result<AppPaths> {
    let root = if crate::use_dev_root() {
        std::env::current_dir()?.join(".dev")
    } else {
        let program_data = std::env::var_os("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        program_data.join("clash-rs-desktop")
    };

    Ok(AppPaths::from_program_data(root))
}

pub(crate) fn app_pipe_path() -> String {
    r"\\.\pipe\clash-rs-desktop-core-local".to_owned()
}

pub(crate) fn global_override_path(paths: &AppPaths) -> PathBuf {
    paths.program_data.join("global-override.yaml")
}
