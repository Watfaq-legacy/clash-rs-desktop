# Choclite for Desktop

A one-switch desktop client for [clash-rs](https://github.com/Watfaq/clash-rs). Paste a subscription (or click a provider's import link), press the button, done. Power-user options live one level down.

Windows and Linux. Tauri 2 + Svelte 5 over a Rust core.

| Level | What you see |
|---|---|
| Home | power switch, live speed, current node, subscription usage/expiry, quiet hints when something needs attention |
| Settings | routing mode, launch at login, auto-connect, subscription management and auto-update, theme, core updates |
| 网络 | 不走代理的地址 (bypass list; LAN and local domains handled automatically), 网络诊断 |
| 高级 | 增强模式 (TUN) incl. IPv6 capture, 网站规则 (site → proxy/direct/block), 自定义配置 (raw YAML), 运行日志 |

## Develop

Requirements: Rust 1.95+, Node 20+, pnpm. Linux also needs the [Tauri system packages](https://tauri.app/start/prerequisites/#linux).

```powershell
cd app
pnpm install
pnpm tauri dev
```

On first connect the app downloads the latest `clash-rs` release. To use a local build instead set `CLASH_RS_DESKTOP_CORE` to its path. For TUN mode in dev, build the service too: `cargo build -p clash-rs-desktop-service`.

Checks:

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets
cd app; pnpm check
```

Packaging: `pnpm bundle:windows` (NSIS, includes the service) or `pnpm bundle:linux` (deb, AppImage).

## How it works

- **System proxy mode** (default): the core runs as a child of the app (in a kill-on-close job) and the OS proxy points at it. No admin rights. Programs that ignore the system proxy are detected and the user is offered 增强模式.
- **增强模式 (TUN)**: a small privileged service (`ChocliteService` on Windows, `choclite.service` on Linux) runs the core with a TUN device. Installed once with one UAC / polkit prompt. The service only runs its own core binary and sanitises configs before use.
- **Subscriptions**: Clash YAML, base64 link lists, single share links (`ss://`, `trojan://`, `vmess://`, `vless://`, `hysteria2://`) or a local file. Several subscriptions can be merged: their nodes join the primary's groups and auto-selection spans all of them.
- **Config layers**: subscription → custom config → bypass/site rules → app settings. App settings always win. Changes hot-reload; a config the core can't run is rolled back instead of dropping the connection.
- **Local networks**: private ranges, `.lan`/`.local`/`home.arpa` and the machine's DNS search suffixes (DHCP, company VPN, Tailscale) go direct; under TUN they are resolved by the system's own DNS.
- **Safety**: the original system proxy is snapshotted and restored on disconnect, core crash, quit, and the next launch after a crash. Node picks are remembered per subscription.
- **Diagnostics**: tells local-network, node, route (another VPN, routing loops), DNS, IPv6 and system-proxy problems apart, with one-click fixes. Uses native OS APIs only.
- **Logs**: core output and app events in rotating files (≈6 MB max).

Flag glyphs: [Twemoji](https://github.com/twitter/twemoji) country flags, CC-BY 4.0. TUN on Windows uses [Wintun](https://www.wintun.net/) (downloaded at service install, pinned by hash).
