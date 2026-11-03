use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CoreUpdateSettings, SystemProxyTarget, UpdateChannel, WindowsArch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub updates: CoreUpdateSettings,
    pub system_proxy: SystemProxySettings,
    pub tun: TunLaunchPolicy,
    #[serde(default)]
    pub connect_mode: ConnectMode,
    #[serde(default)]
    pub routing_mode: RoutingMode,
    /// Connect as soon as the app starts.
    #[serde(default)]
    pub auto_connect: bool,
    /// Simple per-site rules (L2), applied before the subscription's rules.
    #[serde(default)]
    pub custom_rules: Vec<crate::generate::CustomRule>,
    /// Re-download remote profiles on the provider's interval.
    #[serde(default = "default_true")]
    pub auto_update_subscriptions: bool,
    /// "不走代理的地址": domains / IP ranges that always go direct.
    #[serde(default)]
    pub bypass: Vec<String>,
    /// Programs the user chose not to be warned about bypassing the proxy.
    #[serde(default)]
    pub ignored_leak_apps: Vec<String>,
    /// Give the TUN device an IPv6 address so IPv6 traffic is captured too.
    #[serde(default)]
    pub tun_ipv6: bool,
    /// Last node picked per selector group, per profile id. Restored after
    /// every connect/reload, independent of the core's own cache (which
    /// differs between system-proxy and TUN mode).
    #[serde(default)]
    pub selections: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            updates: CoreUpdateSettings::default(),
            system_proxy: SystemProxySettings::default(),
            tun: TunLaunchPolicy::default(),
            connect_mode: ConnectMode::default(),
            routing_mode: RoutingMode::default(),
            auto_connect: false,
            custom_rules: Vec::new(),
            auto_update_subscriptions: true,
            bypass: Vec::new(),
            ignored_leak_apps: Vec::new(),
            tun_ipv6: false,
            selections: Default::default(),
        }
    }
}

/// How traffic reaches the core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectMode {
    /// Set the OS proxy to the core's mixed port. No elevation needed.
    #[default]
    SystemProxy,
    /// Capture all traffic with a TUN device. Needs the privileged service.
    Tun,
}

/// Clash `mode`: which traffic goes through the proxy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingMode {
    #[default]
    Rule,
    Global,
    Direct,
}

impl RoutingMode {
    pub fn as_clash(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Global => "global",
            Self::Direct => "direct",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProxySettings {
    pub enabled: bool,
    pub target: SystemProxyTarget,
    pub restore_on_exit: bool,
}

impl Default for SystemProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target: SystemProxyTarget::loopback_mixed_port(7890),
            restore_on_exit: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TunLaunchPolicy {
    RequireService,
    DenyUntilServiceInstalled,
}

impl Default for TunLaunchPolicy {
    fn default() -> Self {
        Self::RequireService
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
    settings: AppSettings,
}

impl SettingsStore {
    pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let settings = if path.exists() {
            let contents = fs::read_to_string(&path)?;
            serde_json::from_str(contents.trim_start_matches('\u{feff}'))?
        } else {
            AppSettings::default()
        };

        Ok(Self { path, settings })
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.path, serde_json::to_string_pretty(&self.settings)?)?;
        Ok(())
    }
}

impl Default for CoreUpdateSettings {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Stable,
            architecture: WindowsArch::detect(),
            auto_check: true,
            auto_download: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::SettingsStore;

    #[test]
    fn settings_store_persists_defaults() {
        let path = unique_temp_dir().join("settings.json");
        let store = SettingsStore::load(&path).unwrap();
        store.save().unwrap();

        let loaded = SettingsStore::load(&path).unwrap();
        assert!(loaded.settings().updates.auto_check);
        assert_eq!(loaded.settings().system_proxy.target.port, 7890);
    }

    fn unique_temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(Path::new(&format!("clash-rs-settings-test-{nonce}")))
    }
}
