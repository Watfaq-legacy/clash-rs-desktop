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
            bypass: vec![
                "localhost".to_owned(),
                "127.*".to_owned(),
                "<local>".to_owned(),
            ],
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

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsSystemProxy;

impl SystemProxyBackend for WindowsSystemProxy {
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
    use std::process::Command;

    use anyhow::Context as _;

    use super::{SystemProxySnapshot, SystemProxyTarget};

    const INTERNET_SETTINGS_KEY: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    pub fn snapshot() -> anyhow::Result<SystemProxySnapshot> {
        let enabled = query_value("ProxyEnable")?
            .as_deref()
            .is_some_and(|value| matches!(value, "0x1" | "1"));

        Ok(SystemProxySnapshot {
            enabled,
            server: query_value("ProxyServer")?,
            bypass: query_value("ProxyOverride")?,
        })
    }

    pub fn apply(target: &SystemProxyTarget) -> anyhow::Result<()> {
        reg_add("ProxyEnable", "REG_DWORD", "1")?;
        reg_add("ProxyServer", "REG_SZ", &target.server())?;
        reg_add("ProxyOverride", "REG_SZ", &target.bypass_list())?;
        Ok(())
    }

    pub fn restore(snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        reg_add(
            "ProxyEnable",
            "REG_DWORD",
            if snapshot.enabled { "1" } else { "0" },
        )?;

        restore_optional_string("ProxyServer", snapshot.server.as_deref())?;
        restore_optional_string("ProxyOverride", snapshot.bypass.as_deref())?;
        Ok(())
    }

    fn restore_optional_string(name: &str, value: Option<&str>) -> anyhow::Result<()> {
        match value {
            Some(value) => reg_add(name, "REG_SZ", value),
            None => reg_delete(name),
        }
    }

    fn query_value(name: &str) -> anyhow::Result<Option<String>> {
        let output = Command::new("reg")
            .args(["query", INTERNET_SETTINGS_KEY, "/v", name])
            .output()
            .with_context(|| format!("failed to query Windows proxy registry value {name}"))?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .find_map(|line| parse_reg_value_line(line, name)))
    }

    fn reg_add(name: &str, kind: &str, value: &str) -> anyhow::Result<()> {
        let status = Command::new("reg")
            .args([
                "add",
                INTERNET_SETTINGS_KEY,
                "/v",
                name,
                "/t",
                kind,
                "/d",
                value,
                "/f",
            ])
            .status()
            .with_context(|| format!("failed to set Windows proxy registry value {name}"))?;

        if !status.success() {
            anyhow::bail!("failed to set Windows proxy registry value {name}: {status}");
        }

        Ok(())
    }

    fn reg_delete(name: &str) -> anyhow::Result<()> {
        let status = Command::new("reg")
            .args(["delete", INTERNET_SETTINGS_KEY, "/v", name, "/f"])
            .status()
            .with_context(|| format!("failed to delete Windows proxy registry value {name}"))?;

        if !status.success() {
            anyhow::bail!("failed to delete Windows proxy registry value {name}: {status}");
        }

        Ok(())
    }

    fn parse_reg_value_line(line: &str, name: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with(name) {
            return None;
        }

        let mut parts = trimmed
            .splitn(3, char::is_whitespace)
            .filter(|part| !part.is_empty());
        let _name = parts.next()?;
        let _kind = parts.next()?;
        parts.next().map(str::trim).map(str::to_owned)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{SystemProxySnapshot, SystemProxyTarget};

    pub fn snapshot() -> anyhow::Result<SystemProxySnapshot> {
        anyhow::bail!("system proxy management is only implemented on Windows")
    }

    pub fn apply(_target: &SystemProxyTarget) -> anyhow::Result<()> {
        anyhow::bail!("system proxy management is only implemented on Windows")
    }

    pub fn restore(_snapshot: &SystemProxySnapshot) -> anyhow::Result<()> {
        anyhow::bail!("system proxy management is only implemented on Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::SystemProxyTarget;

    #[test]
    fn loopback_target_formats_server_and_bypass() {
        let target = SystemProxyTarget::loopback_mixed_port(7890);

        assert_eq!(target.server(), "127.0.0.1:7890");
        assert_eq!(target.bypass_list(), "localhost;127.*;<local>");
    }
}
