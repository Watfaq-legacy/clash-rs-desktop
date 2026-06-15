#[cfg(all(windows, feature = "reactor"))]
mod app;
#[cfg(all(windows, feature = "reactor"))]
mod controller_actions;
#[cfg(all(windows, feature = "reactor"))]
mod controller_endpoint;
#[cfg(all(windows, feature = "reactor"))]
mod controller_models;
#[cfg(all(windows, feature = "reactor"))]
mod paths;
#[cfg(all(windows, feature = "reactor"))]
mod profile_actions;
#[cfg(all(windows, feature = "reactor"))]
mod runtime_actions;
#[cfg(all(windows, feature = "reactor"))]
mod service_actions;
#[cfg(all(windows, feature = "reactor"))]
mod settings_actions;
#[cfg(all(windows, feature = "reactor"))]
mod update_actions;
#[cfg(all(windows, feature = "reactor"))]
mod views;

#[cfg(all(windows, feature = "reactor"))]
use windows_reactor::*;

#[cfg(all(windows, feature = "reactor"))]
fn main() -> Result<()> {
    App::new()
        .title("clash-rs desktop")
        .inner_size(1180.0, 760.0)
        .render(app::app)
}

#[cfg(not(all(windows, feature = "reactor")))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    eprintln!("clash-rs-desktop ships as a GUI app.");
    eprintln!("build and run with: cargo run -p clash-rs-desktop-ui --features reactor");
    if use_dev_root() {
        eprintln!("--dev detected: GUI will use workspace .dev root when reactor build is used.");
    }
    anyhow::bail!("reactor feature is required for the shipped GUI app")
}

#[cfg(any(
    all(windows, feature = "reactor"),
    not(all(windows, feature = "reactor"))
))]
fn use_dev_root() -> bool {
    std::env::args().any(|arg| arg == "--dev")
}
