//! "网络诊断": read-only checks that turn "the internet is broken" into a
//! plain-language cause — local network vs. node, routes stolen by another
//! VPN, routing loops, DNS not captured, IPv6 leaking around TUN, a system
//! proxy someone else overwrote, leftovers from a crash.
//!
//! Collection (`collect`) is platform-specific; the verdicts (`assess`) are
//! pure so they can be tested.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Ok,
    Info,
    Warn,
    Error,
}

/// One-click remedies the shell knows how to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fix {
    Reconnect,
    ReapplySystemProxy,
    RestoreSystemProxy,
    FlushDns,
    EnableTun,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: &'static str,
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub fix: Option<Fix>,
}

impl Finding {
    fn new(id: &'static str, level: Level, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id,
            level,
            title: title.into(),
            detail: detail.into(),
            fix: None,
        }
    }

    fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

/// What the app knows about its own state.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// `Some(tun)` while connected.
    pub connected_tun: Option<bool>,
    pub mixed_port: Option<u16>,
    /// Proxy servers of the running config (hosts or IPs).
    pub proxy_servers: Vec<String>,
    /// Delay through the current node / direct, `None` = failed.
    pub proxy_delay: Option<Option<u64>>,
    pub direct_delay: Option<Option<u64>>,
    /// Connections that entered through TUN and target a proxy server.
    pub looping_connections: usize,
    /// "全部直连": nodes aren't used, so a dead node isn't a problem.
    pub direct_mode: bool,
}

/// What the OS says.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemState {
    /// Interface carrying traffic to a public IPv4 / IPv6 address.
    pub route4: Option<String>,
    pub route6: Option<String>,
    pub tun_name: Option<String>,
    pub tun_up: bool,
    pub tun_ips: Vec<String>,
    /// Other active VPN/tunnel adapters: "name (description)".
    pub other_vpns: Vec<String>,
    pub has_global_ipv6: bool,
    /// What the OS resolver returned for a public name.
    pub resolved: Option<String>,
    pub proxy_enabled: bool,
    pub proxy_server: Option<String>,
    /// Whether something listens on the mixed port.
    pub port_listening: bool,
    /// Basic reachability without any proxy (raw TCP to public DNS).
    pub internet_reachable: bool,
}

pub fn assess(ctx: &Context, sys: &SystemState) -> Vec<Finding> {
    let mut out = Vec::new();
    match ctx.connected_tun {
        None => assess_disconnected(ctx, sys, &mut out),
        Some(false) => assess_system_proxy(ctx, sys, &mut out),
        Some(true) => assess_tun(ctx, sys, &mut out),
    }
    if ctx.connected_tun.is_some() {
        assess_reachability(ctx, &mut out);
    }
    out.sort_by(|a, b| b.level.cmp(&a.level));
    if out.iter().all(|f| f.level <= Level::Info) {
        out.insert(0, Finding::new("all-good", Level::Ok, "没有发现问题", "网络和代理工作正常。"));
    }
    out
}

fn ours(proxy_server: Option<&str>, port: Option<u16>) -> bool {
    let Some(server) = proxy_server else { return false };
    let local = server.contains("127.0.0.1") || server.contains("localhost");
    local && port.is_none_or(|p| server.contains(&format!(":{p}")))
}

fn assess_disconnected(_ctx: &Context, sys: &SystemState, out: &mut Vec<Finding>) {
    if !sys.internet_reachable {
        out.push(Finding::new(
            "no-network",
            Level::Error,
            "电脑本身没有连上网络",
            "没有连接 Choclite 时也无法访问互联网。请检查 Wi-Fi 或网线，与 Choclite 无关。",
        ));
    }
    // A local proxy left switched on with nothing listening breaks every app.
    if sys.proxy_enabled
        && sys.proxy_server.as_deref().is_some_and(|s| s.contains("127.0.0.1") || s.contains("localhost"))
        && !sys.port_listening
    {
        out.push(
            Finding::new(
                "stale-proxy",
                Level::Error,
                "系统代理指向一个没有运行的程序",
                format!(
                    "系统代理被设置为 {}，但那里没有程序在运行，所以浏览器打不开网页。可能是代理软件异常退出后没有恢复设置。",
                    sys.proxy_server.as_deref().unwrap_or_default()
                ),
            )
            .fix(Fix::RestoreSystemProxy),
        );
    }
    if let Some(name) = &sys.tun_name {
        out.push(
            Finding::new(
                "stale-tun",
                Level::Warn,
                "增强模式的虚拟网卡还在",
                format!("未连接时网卡 {name} 仍然存在，可能会影响上网。点击修复会停止它。"),
            )
            .fix(Fix::Reconnect),
        );
    }
}

fn assess_system_proxy(ctx: &Context, sys: &SystemState, out: &mut Vec<Finding>) {
    if !sys.proxy_enabled || !ours(sys.proxy_server.as_deref(), ctx.mixed_port) {
        let now = match (sys.proxy_enabled, &sys.proxy_server) {
            (false, _) => "已被关闭".to_owned(),
            (true, Some(server)) => format!("被改成了 {server}"),
            (true, None) => "被修改".to_owned(),
        };
        out.push(
            Finding::new(
                "proxy-changed",
                Level::Error,
                "系统代理被其他程序修改了",
                format!("系统代理设置{now}，浏览器的流量不再经过 Choclite。常见原因是同时开着其他代理软件。"),
            )
            .fix(Fix::ReapplySystemProxy),
        );
    }
    if !sys.port_listening {
        out.push(
            Finding::new(
                "port-dead",
                Level::Error,
                "代理端口没有响应",
                "Choclite 的本地代理端口没有在监听，浏览器无法通过它上网。",
            )
            .fix(Fix::Reconnect),
        );
    }
    if !sys.other_vpns.is_empty() {
        out.push(Finding::new(
            "other-vpn",
            Level::Info,
            "检测到其他 VPN",
            format!("{} 正在运行。如果网络不稳定，可以先关闭它们再试。", sys.other_vpns.join("、")),
        ));
    }
}

fn assess_tun(ctx: &Context, sys: &SystemState, out: &mut Vec<Finding>) {
    let Some(tun) = &sys.tun_name else {
        out.push(
            Finding::new(
                "tun-missing",
                Level::Error,
                "增强模式的虚拟网卡不存在",
                "增强模式已开启，但系统里找不到它的虚拟网卡，流量没有被接管。",
            )
            .fix(Fix::Reconnect),
        );
        return;
    };
    if !sys.tun_up || sys.tun_ips.is_empty() {
        out.push(
            Finding::new(
                "tun-down",
                Level::Error,
                "虚拟网卡没有正常工作",
                format!("网卡 {tun} 没有启用或没有分配地址。"),
            )
            .fix(Fix::Reconnect),
        );
    }

    match &sys.route4 {
        Some(route) if route.eq_ignore_ascii_case(tun) => {}
        Some(route) => {
            let culprit = sys
                .other_vpns
                .iter()
                .find(|vpn| vpn.starts_with(route.as_str()))
                .cloned()
                .unwrap_or_else(|| route.clone());
            out.push(
                Finding::new(
                    "route-stolen",
                    Level::Error,
                    "上网流量没有经过增强模式",
                    format!("系统把访问互联网的流量交给了「{culprit}」，而不是 Choclite。通常是另一个 VPN 抢占了默认路由；关闭它后点击修复重新连接。"),
                )
                .fix(Fix::Reconnect),
            );
        }
        None => out.push(Finding::new(
            "route-none",
            Level::Error,
            "没有可用的网络路由",
            "系统找不到访问互联网的路径。请检查网络连接。",
        )),
    }

    if ctx.looping_connections > 0 {
        out.push(
            Finding::new(
                "route-loop",
                Level::Error,
                "检测到路由环路",
                format!(
                    "有 {} 个连接把代理服务器自己的流量又送回了 Choclite，形成死循环，会导致断网或越来越卡。通常是其他 VPN 或网络软件改动了路由；点击修复会重新建立连接。",
                    ctx.looping_connections
                ),
            )
            .fix(Fix::Reconnect),
        );
    }

    if let Some(resolved) = &sys.resolved
        && let Ok(ip) = resolved.parse::<IpAddr>()
        && !is_fake_ip(ip)
    {
        out.push(
            Finding::new(
                "dns-bypass",
                Level::Warn,
                "部分 DNS 查询没有经过 Choclite",
                "系统的域名解析绕过了增强模式。可能是浏览器开启了「安全 DNS」，或者其他网卡的 DNS 优先级更高。分流规则可能因此不准确。",
            )
            .fix(Fix::FlushDns),
        );
    }

    if sys.has_global_ipv6
        && sys
            .route6
            .as_deref()
            .is_some_and(|route| !route.eq_ignore_ascii_case(tun))
    {
        out.push(Finding::new(
            "ipv6-bypass",
            Level::Warn,
            "IPv6 流量会绕过增强模式",
            "你的网络有公网 IPv6，但增强模式只接管了 IPv4。支持 IPv6 的网站可能直接连接，不经过代理。",
        ));
    }

    if !sys.other_vpns.is_empty() {
        out.push(Finding::new(
            "other-vpn",
            Level::Warn,
            "检测到其他 VPN 或虚拟网卡",
            format!("{} 正在运行，可能与增强模式争抢路由。如果遇到断网，先关闭它们。", sys.other_vpns.join("、")),
        ));
    }
}

fn assess_reachability(ctx: &Context, out: &mut Vec<Finding>) {
    let direct_ok = matches!(ctx.direct_delay, Some(Some(_)));
    let proxy = ctx.proxy_delay;
    match (direct_ok, proxy) {
        (false, _) if ctx.direct_delay.is_some() => out.push(Finding::new(
            "local-down",
            Level::Error,
            "本地网络无法访问互联网",
            "连直连都无法访问互联网，问题在你的网络（Wi-Fi、宽带或路由器），不是节点。",
        )),
        (true, Some(None)) if ctx.direct_mode => out.push(Finding::new(
            "node-down",
            Level::Info,
            "当前节点无法连接",
            "现在是「全部直连」模式，不经过节点，所以不影响上网。切换到其他模式前，建议先换一个可用的节点。",
        )),
        (true, Some(None)) => out.push(Finding::new(
            "node-down",
            Level::Error,
            "当前节点无法连接",
            "本地网络正常，但通过当前节点无法访问。可以在「当前节点」里换一个节点，或者更新订阅。",
        )),
        (true, Some(Some(ms))) => out.push(Finding::new(
            "speed",
            Level::Info,
            "连接正常",
            match ctx.direct_delay {
                Some(Some(direct)) => format!("节点延迟 {ms} ms，直连延迟 {direct} ms。"),
                _ => format!("节点延迟 {ms} ms。"),
            },
        )),
        _ => {}
    }
}

/// Fake-IP range clash hands out (198.18.0.0/15).
fn is_fake_ip(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18)
}

/// Gather OS facts. With `routes`, also adapters/routes/DNS (a few seconds
/// on Windows); without, only the cheap proxy/port/reachability checks.
pub fn collect(mixed_port: Option<u16>, routes: bool) -> SystemState {
    let mut sys = if routes { platform::collect() } else { SystemState::default() };
    let proxy = crate::PlatformSystemProxy;
    if let Ok(snapshot) = crate::SystemProxyBackend::snapshot(&proxy) {
        sys.proxy_enabled = snapshot.enabled;
        sys.proxy_server = snapshot.server;
    }
    let port = mixed_port.or_else(|| {
        sys.proxy_server
            .as_deref()
            .and_then(|s| s.rsplit(':').next())
            .and_then(|p| p.parse().ok())
    });
    sys.port_listening = port.is_some_and(|p| {
        std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], p).into(), std::time::Duration::from_millis(500)).is_ok()
    });
    sys.internet_reachable = ["223.5.5.5:53", "1.1.1.1:53", "8.8.8.8:53"].iter().any(|addr| {
        std::net::TcpStream::connect_timeout(&addr.parse().unwrap(), std::time::Duration::from_millis(1500)).is_ok()
    });
    sys
}

#[cfg(windows)]
mod platform {
    use super::SystemState;

    /// One PowerShell round-trip; the NetTCPIP cmdlets answer route
    /// questions exactly as the OS would route a packet.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$r4 = (Find-NetRoute -RemoteIPAddress 1.1.1.1 | Where-Object InterfaceAlias | Select-Object -First 1).InterfaceAlias
$r6 = (Find-NetRoute -RemoteIPAddress 2606:4700:4700::1111 | Where-Object InterfaceAlias | Select-Object -First 1).InterfaceAlias
$tun = Get-NetAdapter -IncludeHidden | Where-Object Name -like 'utun*' | Select-Object -First 1
$tunIps = @()
if ($tun) { $tunIps = @((Get-NetIPAddress -InterfaceIndex $tun.ifIndex -AddressFamily IPv4).IPAddress) }
$pattern = 'WireGuard|Wintun|TAP-|Tailscale|ZeroTier|OpenVPN|AnyConnect|Cisco|Fortinet|GlobalProtect|PANGP|Juniper|Pulse Secure|SonicWall|Mullvad|NordLynx|ProtonVPN|ExpressVPN|Clash|sing-box|Hiddify'
$vpn = @(Get-NetAdapter | Where-Object { $_.Status -eq 'Up' -and $_.Name -notlike 'utun*' -and ($_.InterfaceDescription -match $pattern -or $_.Name -match $pattern) } | ForEach-Object { $_.Name + ' (' + $_.InterfaceDescription + ')' })
$v6 = [bool](Get-NetIPAddress -AddressFamily IPv6 | Where-Object { $_.PrefixOrigin -in 'RouterAdvertisement','Dhcp' -and $_.IPAddress -match '^[23]' })
$dns = (Resolve-DnsName www.example.com -Type A -DnsOnly -QuickTimeout | Where-Object IPAddress | Select-Object -First 1).IPAddress
@{ route4 = $r4; route6 = $r6; tunName = $tun.Name; tunUp = ($tun.Status -eq 'Up'); tunIps = $tunIps; otherVpns = $vpn; hasGlobalIpv6 = $v6; resolved = $dns } | ConvertTo-Json -Compress
"#;

    pub fn collect() -> SystemState {
        use std::os::windows::process::CommandExt as _;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output();
        output
            .ok()
            .and_then(|out| serde_json::from_slice(&out.stdout).ok())
            .unwrap_or_default()
    }
}

#[cfg(not(windows))]
mod platform {
    use std::process::Command;

    use super::SystemState;

    fn run(program: &str, args: &[&str]) -> String {
        Command::new(program)
            .args(args)
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
    }

    /// `ip route get` prints "... dev <name> ...".
    fn route_dev(args: &[&str]) -> Option<String> {
        let text = run("ip", args);
        let mut words = text.split_whitespace();
        words.find(|w| *w == "dev")?;
        words.next().map(str::to_owned)
    }

    pub fn collect() -> SystemState {
        let mut sys = SystemState {
            route4: route_dev(&["route", "get", "1.1.1.1"]),
            route6: route_dev(&["-6", "route", "get", "2606:4700:4700::1111"]),
            ..Default::default()
        };
        for line in run("ip", &["-br", "addr"]).lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let Some(name) = fields.first() else { continue };
            if name.starts_with("utun") {
                sys.tun_name = Some((*name).to_owned());
                sys.tun_up = fields.get(1).is_some_and(|s| *s != "DOWN");
                sys.tun_ips = fields[2..].iter().filter(|a| a.contains('.')).map(|a| (*a).to_owned()).collect();
            } else if ["wg", "tailscale", "zt", "tun", "tap", "nordlynx", "proton"].iter().any(|p| name.starts_with(p)) {
                sys.other_vpns.push((*name).to_owned());
            }
        }
        sys.has_global_ipv6 = !run("ip", &["-6", "addr", "show", "scope", "global"]).trim().is_empty();
        sys.resolved = run("getent", &["ahostsv4", "www.example.com"])
            .split_whitespace()
            .next()
            .map(str::to_owned);
        sys
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, Fix, Level, SystemState, assess};

    fn ids(findings: &[super::Finding]) -> Vec<&str> {
        findings.iter().map(|f| f.id).collect()
    }

    fn healthy_tun() -> SystemState {
        SystemState {
            route4: Some("utun1989".into()),
            tun_name: Some("utun1989".into()),
            tun_up: true,
            tun_ips: vec!["198.18.0.1".into()],
            resolved: Some("198.18.0.7".into()),
            internet_reachable: true,
            ..Default::default()
        }
    }

    fn tun_ctx() -> Context {
        Context {
            connected_tun: Some(true),
            direct_delay: Some(Some(20)),
            proxy_delay: Some(Some(80)),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_tun_reports_all_good() {
        let findings = assess(&tun_ctx(), &healthy_tun());
        assert_eq!(findings[0].level, Level::Ok);
        assert!(ids(&findings).contains(&"speed"));
    }

    #[test]
    fn stolen_route_names_the_other_vpn() {
        let sys = SystemState {
            route4: Some("WireGuard Tunnel".into()),
            other_vpns: vec!["WireGuard Tunnel (WireGuard Tunnel)".into()],
            ..healthy_tun()
        };
        let findings = assess(&tun_ctx(), &sys);
        let stolen = findings.iter().find(|f| f.id == "route-stolen").unwrap();
        assert_eq!(stolen.level, Level::Error);
        assert!(stolen.detail.contains("WireGuard"));
        assert_eq!(findings[0].level, Level::Error, "errors sort first");
    }

    #[test]
    fn loops_dns_bypass_and_ipv6_are_flagged() {
        let ctx = Context {
            looping_connections: 12,
            ..tun_ctx()
        };
        let sys = SystemState {
            resolved: Some("93.184.216.34".into()),
            has_global_ipv6: true,
            route6: Some("Wi-Fi".into()),
            ..healthy_tun()
        };
        let findings = assess(&ctx, &sys);
        for id in ["route-loop", "dns-bypass", "ipv6-bypass"] {
            assert!(ids(&findings).contains(&id), "missing {id}");
        }
        assert_eq!(findings.iter().find(|f| f.id == "route-loop").unwrap().fix, Some(Fix::Reconnect));
    }

    #[test]
    fn local_network_vs_node_failures_are_told_apart() {
        let local_down = Context {
            direct_delay: Some(None),
            proxy_delay: Some(None),
            ..tun_ctx()
        };
        assert!(ids(&assess(&local_down, &healthy_tun())).contains(&"local-down"));

        let node_down = Context {
            proxy_delay: Some(None),
            ..tun_ctx()
        };
        let findings = assess(&node_down, &healthy_tun());
        assert!(ids(&findings).contains(&"node-down"));
        assert!(!ids(&findings).contains(&"local-down"));
    }

    #[test]
    fn system_proxy_overwritten_or_stale() {
        let ctx = Context {
            connected_tun: Some(false),
            mixed_port: Some(7890),
            ..Default::default()
        };
        let sys = SystemState {
            proxy_enabled: true,
            proxy_server: Some("127.0.0.1:7897".into()),
            port_listening: true,
            internet_reachable: true,
            ..Default::default()
        };
        let findings = assess(&ctx, &sys);
        assert_eq!(findings[0].id, "proxy-changed");
        assert_eq!(findings[0].fix, Some(Fix::ReapplySystemProxy));

        // Disconnected, proxy still pointing at a dead local port.
        let stale = SystemState {
            port_listening: false,
            ..sys
        };
        let findings = assess(&Context::default(), &stale);
        assert!(ids(&findings).contains(&"stale-proxy"));
    }
}
