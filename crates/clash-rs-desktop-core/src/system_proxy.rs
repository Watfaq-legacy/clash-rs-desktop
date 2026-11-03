use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemProxyTarget {
    pub host: String,
    pub port: u16,
    pub bypass: Vec<String>,
}

impl SystemProxyTarget {
    pub fn loopback_mixed_port(port: u16) -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port,
            // Loopback and private LAN ranges stay direct (routers, NAS, ...).
            bypass: [
                "localhost",
                "127.*",
                "10.*",
                "172.16.*",
                "172.17.*",
                "172.18.*",
                "172.19.*",
                "172.20.*",
                "172.21.*",
                "172.22.*",
                "172.23.*",
                "172.24.*",
                "172.25.*",
                "172.26.*",
                "172.27.*",
                "172.28.*",
                "172.29.*",
                "172.30.*",
                "172.31.*",
                "192.168.*",
                "<local>",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }

    pub fn server(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn bypass_list(&self) -> String {
        self.bypass.join(";")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemProxySnapshot {
    pub enabled: bool,
    pub server: Option<String>,
    pub bypass: Option<String>,
}

pub trait SystemProxyBackend {
    fn snapshot(&self) -> anyhow::Result<SystemProxySnapshot>;
    fn apply(&self, target: &SystemProxyTarget) -> anyhow::Result<()>;
    fn restore(&self, snapshot: &SystemProxySnapshot) -> anyhow::Result<()>;
}

/// WinINet on Windows, gsettings on Linux.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformSystemProxy;

impl SystemProxyBackend for PlatformSystemProxy {
    fn snapshot(&self) -> anyhow::Result<SystemProxySnapshot> {
        platform::snapshot()
    }

    fn apply(&self, target: &SystemProxyTarget) -> anyhow::Result<()> {
        platform::apply(target)
    }

    fn restore(&self, snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        platform::restore(snapshot)
    }
}

#[cfg(windows)]
mod platform {
    use anyhow::Context as _;
    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR},
            System::Registry::{
                HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
                RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
            },
        },
        core::HSTRING,
    };

    use super::{SystemProxySnapshot, SystemProxyTarget};

    const INTERNET_SETTINGS_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    pub fn snapshot() -> anyhow::Result<SystemProxySnapshot> {
        snapshot_at(INTERNET_SETTINGS_KEY)
    }

    pub fn apply(target: &SystemProxyTarget) -> anyhow::Result<()> {
        apply_at(INTERNET_SETTINGS_KEY, target)?;
        notify_settings_changed();
        Ok(())
    }

    pub fn restore(snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        restore_at(INTERNET_SETTINGS_KEY, snapshot)?;
        notify_settings_changed();
        Ok(())
    }

    // Registry API rather than `reg.exe`: no output parsing (which dropped
    // values and then "restored" them as deleted) and no child processes.
    fn snapshot_at(key: &str) -> anyhow::Result<SystemProxySnapshot> {
        Ok(SystemProxySnapshot {
            enabled: get_dword(key, "ProxyEnable")?.unwrap_or(0) != 0,
            server: get_string(key, "ProxyServer")?,
            bypass: get_string(key, "ProxyOverride")?,
        })
    }

    fn apply_at(key: &str, target: &SystemProxyTarget) -> anyhow::Result<()> {
        set_string(key, "ProxyServer", &target.server())?;
        set_string(key, "ProxyOverride", &target.bypass_list())?;
        // Enable last, so the proxy never turns on with a stale server.
        set_dword(key, "ProxyEnable", 1)
    }

    fn restore_at(key: &str, snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        set_dword(key, "ProxyEnable", u32::from(snapshot.enabled))?;
        for (name, value) in [
            ("ProxyServer", &snapshot.server),
            ("ProxyOverride", &snapshot.bypass),
        ] {
            match value {
                Some(value) => set_string(key, name, value)?,
                None => delete_value(key, name)?,
            }
        }
        Ok(())
    }

    /// Without this, WinINet/browsers keep the old proxy until they restart.
    fn notify_settings_changed() {
        use windows::Win32::Networking::WinInet::{
            INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED, InternetSetOptionW,
        };
        // SAFETY: both options take no buffer.
        unsafe {
            let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
            let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
        }
    }

    fn check(status: WIN32_ERROR, what: &str) -> anyhow::Result<()> {
        status
            .ok()
            .with_context(|| format!("registry {what} failed"))
    }

    fn get_string(key: &str, name: &str) -> anyhow::Result<Option<String>> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut size = 0u32;
        // SAFETY: size query with no buffer, then a buffer of `size` bytes.
        // RRF_RT_REG_SZ guarantees NUL termination.
        unsafe {
            let status = RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            );
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            check(status, "read")?;

            let mut buf = vec![0u16; (size as usize).div_ceil(2)];
            check(
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    &key,
                    &name,
                    RRF_RT_REG_SZ,
                    None,
                    Some(buf.as_mut_ptr().cast()),
                    Some(&mut size),
                ),
                "read",
            )?;
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Ok(Some(String::from_utf16_lossy(&buf[..len])))
        }
    }

    fn get_dword(key: &str, name: &str) -> anyhow::Result<Option<u32>> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: `value` is a 4-byte buffer and `size` says so.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status, "read")?;
        Ok(Some(value))
    }

    fn set_string(key: &str, name: &str, value: &str) -> anyhow::Result<()> {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and its byte length is passed.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(key),
                &HSTRING::from(name),
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                (wide.len() * 2) as u32,
            )
        };
        check(status, "write")
    }

    fn set_dword(key: &str, name: &str, value: u32) -> anyhow::Result<()> {
        // SAFETY: 4-byte buffer, length 4.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(key),
                &HSTRING::from(name),
                REG_DWORD.0,
                Some((&value as *const u32).cast()),
                4,
            )
        };
        check(status, "write")
    }

    fn delete_value(key: &str, name: &str) -> anyhow::Result<()> {
        // SAFETY: plain call with valid wide strings.
        let status = unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name))
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(status, "delete")
    }

    #[cfg(test)]
    mod tests {
        use super::{SystemProxySnapshot, SystemProxyTarget, apply_at, restore_at, snapshot_at};

        /// snapshot → apply → restore on a scratch key must give back exactly
        /// the original values (regression: `:::8899` used to come back empty).
        #[test]
        fn apply_then_restore_round_trips_original_values() {
            let key = format!(r"Software\clash-rs-desktop-test-{}", std::process::id());
            let original = SystemProxySnapshot {
                enabled: false,
                server: Some(":::8899".to_owned()),
                bypass: Some("localhost;127.*;<local>".to_owned()),
            };
            restore_at(&key, &original).unwrap();

            let before = snapshot_at(&key).unwrap();
            assert_eq!(before, original);

            apply_at(&key, &SystemProxyTarget::loopback_mixed_port(7890)).unwrap();
            let applied = snapshot_at(&key).unwrap();
            assert!(applied.enabled);
            assert_eq!(applied.server.as_deref(), Some("127.0.0.1:7890"));

            restore_at(&key, &before).unwrap();
            assert_eq!(snapshot_at(&key).unwrap(), original);

            let empty = SystemProxySnapshot {
                enabled: false,
                server: None,
                bypass: None,
            };
            restore_at(&key, &empty).unwrap();
            assert_eq!(snapshot_at(&key).unwrap(), empty);

            let _ = std::process::Command::new("reg")
                .args(["delete", &format!(r"HKCU\{key}"), "/f"])
                .output();
        }
    }
}

/// Linux desktops: GNOME-compatible `gsettings` (GNOME, Cinnamon, Budgie,
/// Pantheon, ...). The snapshot stores `mode` in `enabled`, `host:port` of
/// the HTTP proxy in `server`, and the raw `ignore-hosts` value in `bypass`.
#[cfg(not(windows))]
mod platform {
    use std::process::Command;

    use anyhow::Context as _;

    use super::{SystemProxySnapshot, SystemProxyTarget};

    const SCHEMA: &str = "org.gnome.system.proxy";

    fn get(schema: &str, key: &str) -> anyhow::Result<String> {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .context(crate::i18n::t(
                "没有找到 gsettings，当前桌面环境暂不支持自动设置系统代理",
                "gsettings not found; this desktop environment doesn't support setting the system proxy automatically",
            ))?;
        anyhow::ensure!(
            output.status.success(),
            "gsettings get {schema} {key} failed"
        );
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn set(schema: &str, key: &str, value: &str) -> anyhow::Result<()> {
        let status = Command::new("gsettings")
            .args(["set", schema, key, value])
            .status()
            .context("failed to run gsettings")?;
        anyhow::ensure!(status.success(), "gsettings set {schema} {key} failed");
        Ok(())
    }

    fn unquote(value: &str) -> String {
        value.trim_matches('\'').to_owned()
    }

    pub fn snapshot() -> anyhow::Result<SystemProxySnapshot> {
        let host = unquote(&get(&format!("{SCHEMA}.http"), "host")?);
        let port = get(&format!("{SCHEMA}.http"), "port")?;
        Ok(SystemProxySnapshot {
            enabled: get(SCHEMA, "mode")? == "'manual'",
            server: (!host.is_empty()).then(|| format!("{host}:{port}")),
            bypass: Some(get(SCHEMA, "ignore-hosts")?),
        })
    }

    pub fn apply(target: &SystemProxyTarget) -> anyhow::Result<()> {
        for scheme in ["http", "https", "socks"] {
            set(
                &format!("{SCHEMA}.{scheme}"),
                "host",
                &format!("'{}'", target.host),
            )?;
            set(
                &format!("{SCHEMA}.{scheme}"),
                "port",
                &target.port.to_string(),
            )?;
        }
        set(
            SCHEMA,
            "ignore-hosts",
            "['localhost', '127.0.0.0/8', '::1', '10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16']",
        )?;
        set(SCHEMA, "mode", "'manual'")
    }

    pub fn restore(snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        let (host, port) = snapshot
            .server
            .as_deref()
            .and_then(|server| server.rsplit_once(':'))
            .unwrap_or(("", "0"));
        for scheme in ["http", "https", "socks"] {
            set(&format!("{SCHEMA}.{scheme}"), "host", &format!("'{host}'"))?;
            set(&format!("{SCHEMA}.{scheme}"), "port", port)?;
        }
        if let Some(bypass) = &snapshot.bypass {
            set(SCHEMA, "ignore-hosts", bypass)?;
        }
        set(
            SCHEMA,
            "mode",
            if snapshot.enabled {
                "'manual'"
            } else {
                "'none'"
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SystemProxyTarget;

    #[test]
    fn loopback_target_formats_server_and_bypass() {
        let target = SystemProxyTarget::loopback_mixed_port(7890);

        assert_eq!(target.server(), "127.0.0.1:7890");
        let bypass = target.bypass_list();
        assert!(bypass.starts_with("localhost;127.*;10.*;172.16.*"));
        assert!(bypass.ends_with(";192.168.*;<local>"));
    }
}
