//! End-to-end smoke test of `Engine` without the UI.
//!
//! ```text
//! cargo run -p clash-rs-desktop-core --example engine_smoke -- <data-dir> <subscription-url> [system-proxy|tun]
//! ```
//!
//! Imports the subscription, connects, prints controller facts, disconnects.

use clash_rs_desktop_core::{AppPaths, ConnectMode, Engine, fetch_subscription};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(url)) = (args.next(), args.next()) else {
        anyhow::bail!("usage: engine_smoke <data-dir> <subscription-url> [system-proxy|tun]");
    };
    let mode = match args.next().as_deref() {
        Some("tun") => ConnectMode::Tun,
        _ => ConnectMode::SystemProxy,
    };

    let mut engine = Engine::load(AppPaths::from_program_data(&dir))?;
    if engine.profiles().active_profile().is_none() {
        let paths = engine.paths().clone();
        let fetched = fetch_subscription(&url).await?;
        let profiles = engine.profiles_mut();
        profiles.import_remote(&paths, fetched)?;
        profiles.save()?;
    }
    let store = engine.settings_mut();
    store.settings_mut().connect_mode = mode;
    store.save()?;

    engine.connect(&|step| println!("step: {step}")).await?;
    let controller = engine.controller().expect("connected");
    println!(
        "connected: core {}",
        controller.get_version().await?.version
    );
    println!(
        "mode: {:?}, port: {:?}",
        engine.connected_mode(),
        engine.mixed_port()
    );
    println!("proxies: {}", controller.get_proxies().await?.proxies.len());

    engine.reload().await?;
    println!("hot reload: ok");

    // Optional: stay connected so the caller can inspect the system.
    if let Some(secs) = std::env::var("SMOKE_HOLD_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
    if std::env::var_os("SMOKE_LOGS").is_some() {
        for line in engine.core_log_tail(40).await {
            println!("core| {line}");
        }
    }

    engine.disconnect().await?;
    println!("disconnected");
    Ok(())
}
