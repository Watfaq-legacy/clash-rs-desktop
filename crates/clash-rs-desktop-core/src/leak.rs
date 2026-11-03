//! Best-effort detection of programs that ignore the system proxy.
//!
//! In system-proxy mode a well-behaved app only talks to 127.0.0.1 (the
//! core). A program holding its own TCP connection to a public address is
//! going around us — typically a game, a launcher or a CLI tool, which
//! 增强模式 (TUN) would cover. UDP (QUIC, games) is invisible here, which is
//! fine: this only powers a gentle hint.

use std::{collections::HashSet, net::IpAddr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Connection {
    pid: u32,
    remote: IpAddr,
    remote_port: u16,
    local_port: u16,
}

pub struct LeakDetector {
    /// Connections that already existed when we connected; established
    /// before the proxy was on, so not evidence of anything.
    baseline: HashSet<Connection>,
    /// Our own processes (app, core).
    ignore_pids: Vec<u32>,
}

impl LeakDetector {
    pub fn start(ignore_pids: Vec<u32>) -> Self {
        Self {
            baseline: platform::tcp_connections().into_iter().collect(),
            ignore_pids,
        }
    }

    /// Executable names (e.g. `Steam.exe`) with new direct connections to
    /// public addresses, excluding OS components.
    pub fn scan(&self) -> Vec<String> {
        let mut apps: Vec<String> = platform::tcp_connections()
            .into_iter()
            .filter(|c| !self.baseline.contains(c))
            .filter(|c| c.pid > 4 && !self.ignore_pids.contains(&c.pid))
            .filter(|c| is_public(c.remote))
            .filter_map(|c| platform::process_image(c.pid))
            .filter(|path| !is_os_component(path))
            .map(|path| display_name(&path))
            .collect();
        apps.sort_by_key(|name| name.to_ascii_lowercase());
        apps.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        apps
    }
}

/// What users recognise: the program's own description ("Example Game
/// Launcher"), else a cleaned file name ("app.EXE.1700….old" → "app").
fn display_name(path: &str) -> String {
    if let Some(description) = platform::file_description(path)
        && !description.trim().is_empty()
    {
        return description.trim().to_owned();
    }
    clean_file_name(path)
}

fn clean_file_name(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_owned();
    let lower = name.to_ascii_lowercase();
    match lower.find(".exe") {
        Some(end) => name[..end].to_owned(),
        None => name,
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

/// Programs shipped with Windows (update, telemetry, store, ...) use WinHTTP
/// or their own stacks by design; warning about them would be noise.
fn is_os_component(path: &str) -> bool {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let path = path.to_ascii_lowercase();
    path.starts_with(&windir.to_ascii_lowercase())
        || path.contains(r"\windowsapps\microsoft.")
        || path.contains(r"\microsoft\edgeupdate\")
}

#[cfg(windows)]
mod platform {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use windows::Win32::{
        Foundation::{CloseHandle, NO_ERROR},
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP_STATE_ESTAB, MIB_TCP6ROW_OWNER_PID,
            MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
        },
        Networking::WinSock::{AF_INET, AF_INET6},
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    };

    use super::Connection;

    /// The raw table for one address family, in a u32-aligned buffer.
    fn table(family: u32) -> Option<Vec<u32>> {
        let mut size = 0u32;
        // SAFETY: first call only reports the needed size; second call
        // fills a buffer of at least that size.
        unsafe {
            let _ = GetExtendedTcpTable(
                None,
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            );
            for _ in 0..3 {
                let mut buf = vec![0u32; (size as usize).div_ceil(4) + 16];
                let mut len = (buf.len() * 4) as u32;
                let ret = GetExtendedTcpTable(
                    Some(buf.as_mut_ptr().cast()),
                    &mut len,
                    false,
                    family,
                    TCP_TABLE_OWNER_PID_CONNECTIONS,
                    0,
                );
                if ret == NO_ERROR.0 {
                    return Some(buf);
                }
                size = len; // table grew between calls; retry
            }
        }
        None
    }

    pub fn tcp_connections() -> Vec<Connection> {
        let mut out = Vec::new();
        let established = MIB_TCP_STATE_ESTAB.0 as u32;
        if let Some(buf) = table(AF_INET.0 as u32) {
            // SAFETY: the buffer holds a MIB_TCPTABLE_OWNER_PID written by
            // the OS; dwNumEntries rows follow the header.
            let rows = unsafe {
                let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
            };
            out.extend(
                rows.iter()
                    .filter(|r: &&MIB_TCPROW_OWNER_PID| r.dwState == established)
                    .map(|r| Connection {
                        pid: r.dwOwningPid,
                        remote: IpAddr::V4(Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes())),
                        remote_port: u16::from_be(r.dwRemotePort as u16),
                        local_port: u16::from_be(r.dwLocalPort as u16),
                    }),
            );
        }
        if let Some(buf) = table(AF_INET6.0 as u32) {
            // SAFETY: as above, for the IPv6 table.
            let rows = unsafe {
                let table = &*(buf.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID);
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
            };
            out.extend(
                rows.iter()
                    .filter(|r: &&MIB_TCP6ROW_OWNER_PID| r.dwState == established)
                    .map(|r| Connection {
                        pid: r.dwOwningPid,
                        remote: IpAddr::V6(Ipv6Addr::from(r.ucRemoteAddr)),
                        remote_port: u16::from_be(r.dwRemotePort as u16),
                        local_port: u16::from_be(r.dwLocalPort as u16),
                    }),
            );
        }
        out
    }

    /// `FileDescription` from the executable's version resource.
    pub fn file_description(path: &str) -> Option<String> {
        use windows::{
            Win32::Storage::FileSystem::{
                GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
            },
            core::HSTRING,
        };
        let path = HSTRING::from(path);
        // SAFETY: the version block is copied into `data`, which outlives
        // every pointer VerQueryValueW hands back into it.
        unsafe {
            let size = GetFileVersionInfoSizeW(&path, None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(&path, None, size, data.as_mut_ptr().cast()).ok()?;

            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            // First translation: (language, codepage) pair.
            let lang = if VerQueryValueW(
                data.as_ptr().cast(),
                &HSTRING::from(r"\VarFileInfo\Translation"),
                &mut ptr,
                &mut len,
            )
            .as_bool()
                && len >= 4
            {
                let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
                format!("{:04x}{:04x}", pair[0], pair[1])
            } else {
                "040904b0".to_owned()
            };

            let key = HSTRING::from(format!(r"\StringFileInfo\{lang}\FileDescription"));
            if !VerQueryValueW(data.as_ptr().cast(), &key, &mut ptr, &mut len).as_bool() || len == 0
            {
                return None;
            }
            let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
            Some(String::from_utf16_lossy(&text[..end]))
        }
    }

    pub fn process_image(pid: u32) -> Option<String> {
        // SAFETY: limited-query handle, closed before returning; the name
        // buffer outlives the call.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            ok.ok()?;
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }
}

/// Not implemented elsewhere yet: Linux desktop apps mostly honour the
/// GNOME proxy or not at all, so a hint there would be noisy.
#[cfg(not(windows))]
mod platform {
    use super::Connection;

    pub fn tcp_connections() -> Vec<Connection> {
        Vec::new()
    }

    pub fn process_image(_pid: u32) -> Option<String> {
        None
    }

    pub fn file_description(_path: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{clean_file_name, is_public};

    #[test]
    fn cleans_odd_executable_names() {
        assert_eq!(
            clean_file_name(r"C:\x\app.EXE.1700000000000000000.old"),
            "app"
        );
        assert_eq!(clean_file_name(r"C:\Games\Steam.exe"), "Steam");
        assert_eq!(clean_file_name("/usr/bin/curl"), "curl");
    }

    #[test]
    fn classifies_addresses() {
        for public in ["1.1.1.1", "142.250.1.1", "2606:4700::1111"] {
            assert!(is_public(public.parse().unwrap()), "{public}");
        }
        for local in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.1",
            "100.100.1.1",
            "198.18.0.1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.1",
        ] {
            assert!(!is_public(local.parse().unwrap()), "{local}");
        }
    }
}
