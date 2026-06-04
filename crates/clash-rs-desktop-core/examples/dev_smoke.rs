use std::{env, path::PathBuf, time::Duration};

use clash_rs_desktop_core::{
    ActiveCore, AppPaths, CoreRuntimeManager, ProfileStore, runtime_options_from_active_profile,
};

const DEFAULT_BINARY_PATH: &str = ".dev/core/clash-rs.exe";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let workspace_root = env::current_dir()?;
    let binary_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join(DEFAULT_BINARY_PATH));

    if !binary_path.exists() {
        anyhow::bail!(
            "clash-rs binary not found at {}. Pass a binary path as the first argument.",
            binary_path.display()
        );
    }

    let metadata_path = workspace_root.join(".dev/core/active.json");
    let active_core = if metadata_path.exists() {
        ActiveCore::load(metadata_path)?
    } else {
        anyhow::bail!("active core metadata not found at .dev/core/active.json");
    };

    let paths = AppPaths::from_program_data(workspace_root.join(".dev"));
    let mut profile_store = ProfileStore::load(paths.profiles_dir.join("profiles.json"))?;
    profile_store.ensure_default_profile(&paths)?;
    profile_store.save()?;

    let mut active_core = active_core;
    active_core.binary_path = binary_path;
    let options = runtime_options_from_active_profile(
        paths,
        active_core,
        &profile_store,
        None,
        format!(r"\\.\pipe\clash-rs-desktop-dev-{}", std::process::id()),
    )?;

    let mut manager = CoreRuntimeManager::new(options);
    manager.start_validated()?;

    let result = manager.wait_until_ready(Duration::from_secs(5)).await;
    let stop_result = manager.stop();

    let version = result?;
    stop_result?;

    println!(
        "clash-rs smoke check ok: version={} commit={}",
        version.version,
        version.commit.as_deref().unwrap_or("unknown")
    );

    Ok(())
}
