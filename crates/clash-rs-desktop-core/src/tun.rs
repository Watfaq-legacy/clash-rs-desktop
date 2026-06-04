use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::process::LaunchRequirements;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TunSettings {
    pub enable: bool,
    pub device_id: Option<String>,
    pub gateway: Option<String>,
    pub gateway_v6: Option<String>,
    pub routes: Vec<String>,
    pub route_all: bool,
    pub mtu: Option<u16>,
    pub dns_hijack: bool,
}

impl TunSettings {
    pub fn launch_requirements(&self) -> LaunchRequirements {
        if self.enable {
            LaunchRequirements::for_tun()
        } else {
            LaunchRequirements::normal()
        }
    }
}

pub fn launch_requirements_from_config(config: &Value) -> LaunchRequirements {
    if config
        .get("tun")
        .and_then(|tun| tun.get("enable"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        LaunchRequirements::for_tun()
    } else {
        LaunchRequirements::normal()
    }
}

#[cfg(test)]
mod tests {
    use serde_yaml::Value;

    use super::launch_requirements_from_config;

    #[test]
    fn enabled_tun_requires_service() {
        let config: Value = serde_yaml::from_str("tun:\n  enable: true\n").unwrap();
        assert!(launch_requirements_from_config(&config).requires_service);
    }

    #[test]
    fn disabled_tun_can_launch_directly() {
        let config: Value = serde_yaml::from_str("tun:\n  enable: false\n").unwrap();
        assert!(!launch_requirements_from_config(&config).requires_service);
    }
}
