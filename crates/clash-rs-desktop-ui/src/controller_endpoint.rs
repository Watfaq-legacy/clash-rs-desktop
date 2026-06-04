
use std::path::PathBuf;

use clash_rs_desktop_core::ClashIpcClientConfig;
use serde::{Deserialize, Serialize};

use crate::paths::{app_pipe_path, reactor_paths};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ControllerEndpointConfig {
    pub(crate) endpoint: String,
    pub(crate) secret: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ControllerEndpointInputs {
    pub(crate) endpoint: String,
    pub(crate) secret: String,
}

impl Default for ControllerEndpointConfig {
    fn default() -> Self {
        Self {
            endpoint: app_pipe_path(),
            secret: String::new(),
        }
    }
}

pub(crate) fn controller_inputs() -> ControllerEndpointInputs {
    let config = load_controller_endpoint_config().unwrap_or_default();
    ControllerEndpointInputs {
        endpoint: config.endpoint,
        secret: config.secret,
    }
}

pub(crate) fn controller_client_config() -> ClashIpcClientConfig {
    let config = load_controller_endpoint_config().unwrap_or_default();
    ClashIpcClientConfig {
        pipe_path: config.endpoint,
        auth_secret: optional_string(&config.secret),
    }
}

pub(crate) fn save_controller_endpoint_text(endpoint: &str, secret: &str) -> String {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return "controller endpoint save failed: endpoint is required".to_owned();
    }

    let config = ControllerEndpointConfig {
        endpoint: endpoint.to_owned(),
        secret: secret.trim().to_owned(),
    };

    match save_controller_endpoint_config(&config) {
        Ok(path) => format!(
            "controller endpoint saved\nendpoint: {}\nsecret: {}\npath: {}",
            config.endpoint,
            if config.secret.is_empty() { "<empty>" } else { "<set>" },
            path.display()
        ),
        Err(error) => format!("controller endpoint save failed: {error}"),
    }
}

fn load_controller_endpoint_config() -> anyhow::Result<ControllerEndpointConfig> {
    let path = controller_endpoint_path()?;
    if !path.exists() {
        return Ok(ControllerEndpointConfig::default());
    }
    let contents = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(contents.trim_start_matches('\u{feff}'))?)
}

fn save_controller_endpoint_config(config: &ControllerEndpointConfig) -> anyhow::Result<PathBuf> {
    let path = controller_endpoint_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(config)?)?;
    Ok(path)
}

fn controller_endpoint_path() -> anyhow::Result<PathBuf> {
    let paths = reactor_paths()?;
    Ok(paths.program_data.join("controller-endpoint.json"))
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}
