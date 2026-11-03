//! What the machine's own network looks like, so local names and addresses
//! keep working when Choclite is on: home routers (`.lan`), NAS boxes,
//! company intranets with split DNS, Tailscale/ZeroTier, ...

use serde::{Deserialize, Serialize};

/// Suffixes that are local by convention (RFC 6762 / 8375 and common
/// router defaults). Always direct, resolved by the system DNS.
pub const LOCAL_SUFFIXES: [&str; 6] = [
    "lan",
    "local",
    "localdomain",
    "home.arpa",
    "internal",
    "intranet",
];

/// Private, link-local and CGNAT ranges (CGNAT also covers Tailscale).
pub const LOCAL_CIDRS: [&str; 9] = [
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "100.64.0.0/10",
    "169.254.0.0/16",
    "127.0.0.0/8",
    "fc00::/7",
    "fe80::/10",
    "::1/128",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalNetwork {
    /// DNS search / connection suffixes configured on this machine (from
    /// DHCP, a company VPN, Tailscale's `*.ts.net`, ...).
    pub search_suffixes: Vec<String>,
    /// The DNS servers the OS was using before we took over.
    pub dns_servers: Vec<String>,
}

impl LocalNetwork {
    pub fn detect() -> Self {
        let mut network = platform::detect();
        network.search_suffixes = normalise(network.search_suffixes);
        network.dns_servers.retain(|server| usable_dns(server));
        network.dns_servers.dedup();
        network
    }

    /// Built-in local suffixes plus the machine's search suffixes.
    pub fn local_suffixes(&self) -> Vec<String> {
        normalise(
            LOCAL_SUFFIXES
                .iter()
                .map(|s| (*s).to_owned())
                .chain(self.search_suffixes.iter().cloned())
                .collect(),
        )
    }
}

fn normalise(mut suffixes: Vec<String>) -> Vec<String> {
    for suffix in &mut suffixes {
        *suffix = suffix.trim().trim_matches('.').to_ascii_lowercase();
    }
    suffixes.retain(|s| !s.is_empty() && s.contains(|c: char| c.is_ascii_alphanumeric()));
    suffixes.sort();
    suffixes.dedup();
    suffixes
}

/// Skip our own TUN DNS (198.18.0.0/15) and unusable placeholders.
fn usable_dns(server: &str) -> bool {
    let Ok(ip) = server.trim().parse::<std::net::IpAddr>() else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !v4.is_unspecified() && !(a == 198 && (b == 18 || b == 19))
        }
        std::net::IpAddr::V6(v6) => !v6.is_unspecified() && !v6.is_loopback(),
    }
}

#[cfg(windows)]
mod platform {
    use windows::{
        Win32::System::Registry::{
            HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW,
            RegGetValueW, RegOpenKeyExW,
        },
        core::{HSTRING, PWSTR},
    };

    use super::LocalNetwork;

    const TCPIP: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";

    pub fn detect() -> LocalNetwork {
        let mut network = LocalNetwork::default();
        let mut push_suffixes = |raw: Option<String>| {
            if let Some(raw) = raw {
                network
                    .search_suffixes
                    .extend(raw.split([',', ' ']).map(str::to_owned));
            }
        };
        push_suffixes(get_string(TCPIP, "SearchList"));
        push_suffixes(get_string(TCPIP, "Domain"));
        push_suffixes(get_string(TCPIP, "DhcpDomain"));

        let interfaces = format!(r"{TCPIP}\Interfaces");
        for id in subkeys(&interfaces) {
            let key = format!(r"{interfaces}\{id}");
            push_suffixes(get_string(&key, "Domain"));
            push_suffixes(get_string(&key, "DhcpDomain"));
            push_suffixes(get_string(&key, "SearchList"));
            for name in ["NameServer", "DhcpNameServer"] {
                if let Some(raw) = get_string(&key, name) {
                    network.dns_servers.extend(
                        raw.split([',', ' '])
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned),
                    );
                }
            }
        }
        network
    }

    fn get_string(key: &str, name: &str) -> Option<String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut size = 0u32;
        // SAFETY: size query, then a buffer of `size` bytes; REG_SZ is
        // NUL-terminated by RRF_RT_REG_SZ.
        unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
            .ok()
            .ok()?;
            let mut buf = vec![0u16; (size as usize).div_ceil(2)];
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
            .ok()
            .ok()?;
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let text = String::from_utf16_lossy(&buf[..len]);
            (!text.trim().is_empty()).then_some(text)
        }
    }

    fn subkeys(path: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut key = HKEY::default();
        // SAFETY: the key is opened read-only and closed below; the name
        // buffer outlives each enumeration call.
        unsafe {
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(path),
                None,
                KEY_READ,
                &mut key,
            )
            .is_err()
            {
                return names;
            }
            for index in 0.. {
                let mut buf = [0u16; 256];
                let mut len = buf.len() as u32;
                if RegEnumKeyExW(
                    key,
                    index,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
                .is_err()
                {
                    break;
                }
                names.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            let _ = RegCloseKey(key);
        }
        names
    }
}

#[cfg(not(windows))]
mod platform {
    use super::LocalNetwork;

    /// `search`/`domain` and `nameserver` lines of resolv.conf. On
    /// systemd-resolved hosts this is the 127.0.0.53 stub, which still
    /// answers local names and is not captured by the TUN.
    pub fn detect() -> LocalNetwork {
        let text = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
        parse_resolv_conf(&text)
    }

    pub(super) fn parse_resolv_conf(text: &str) -> LocalNetwork {
        let mut network = LocalNetwork::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("search" | "domain") => {
                    network.search_suffixes.extend(words.map(str::to_owned))
                }
                Some("nameserver") => network.dns_servers.extend(words.next().map(str::to_owned)),
                _ => {}
            }
        }
        network
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalNetwork, normalise, usable_dns};

    #[test]
    fn suffixes_are_normalised_and_merged_with_builtins() {
        let network = LocalNetwork {
            search_suffixes: normalise(vec![
                ".Corp.Example.com.".into(),
                "".into(),
                "tail1234.ts.net".into(),
            ]),
            dns_servers: vec![],
        };
        let all = network.local_suffixes();
        assert!(all.contains(&"corp.example.com".to_owned()));
        assert!(all.contains(&"tail1234.ts.net".to_owned()));
        assert!(all.contains(&"home.arpa".to_owned()));
    }

    #[test]
    fn our_own_tun_dns_is_not_a_system_dns() {
        assert!(usable_dns("192.0.2.53"));
        assert!(usable_dns("127.0.0.53"));
        assert!(!usable_dns("198.18.0.2"));
        assert!(!usable_dns("0.0.0.0"));
        assert!(!usable_dns("not-an-ip"));
    }

    #[cfg(not(windows))]
    #[test]
    fn parses_resolv_conf() {
        let network = super::platform::parse_resolv_conf(
            "# comment\nnameserver 127.0.0.53\nsearch lan corp.example.com\noptions edns0\n",
        );
        assert_eq!(network.dns_servers, ["127.0.0.53"]);
        assert_eq!(network.search_suffixes, ["lan", "corp.example.com"]);
    }
}
