# Choclite for Desktop

A one-switch desktop client for [clash-rs](https://github.com/Watfaq/clash-rs). Paste a subscription (or click a provider's import link), press the button, done. Power-user options live one level down.

Windows and Linux. Tauri 2 + Svelte 5 over a Rust core.

| Level | What you see |
|---|---|
| Home | power switch, live speed, current node, subscription usage/expiry |
| Settings | system proxy / 增强模式 (TUN), routing mode, launch at login, auto-connect, theme, subscription auto-update, core updates |
| 网站规则 | "this site / IP / program → proxy, direct, block" without writing YAML |
| 自定义配置 / 日志 | raw Clash YAML layered over the subscription; core logs |

## Develop

Requirements: Rust 1.95+, Node 20+, pnpm. Linux also needs the [Tauri system packages](https://tauri.app/start/prerequisites/#linux).

```powershell
cd app
pnpm install
pnpm tauri dev
```

On first connect the app downloads the latest `clash-rs` release. To use a local build instead set `CLASH_RS_DESKTOP_CORE` to its path. For TUN mode in dev, build the service too: `cargo build -p clash-rs-desktop-service`.

Tests and checks:

```powershell
cargo test --workspace
cd app; pnpm check
```

Packaging: `pnpm bundle:windows` (NSIS, includes the service) or `pnpm bundle:linux` (deb, AppImage).

## How it works

- **System proxy mode**: the core runs as a child of the app (in a kill-on-close job) and the OS proxy points at it. No admin rights.
- **增强模式 (TUN)**: a small privileged service (`ChocliteService` on Windows, `choclite.service` on Linux) runs the core with a TUN device. Installed once with one UAC / polkit prompt. The service only runs its own core binary and sanitises configs before use.
- **Config layers**: subscription → custom config → site rules → app settings. Custom config can extend lists (`prepend-rules`, `append-rules`, `prepend-proxies`, `prepend-proxy-groups`, ...). Changes hot-reload without dropping the connection.
- **Safety**: the original system proxy is snapshotted to disk and restored on disconnect, on core crash, on quit, and on the next launch after a crash.
- **Import links**: `choclite://install-config?url=...` always; `clash://install-config?url=...` too when no other client owns `clash://`. Imports always ask for confirmation.

Flag glyphs: [Twemoji](https://github.com/twitter/twemoji) country flags, CC-BY 4.0. TUN on Windows uses [Wintun](https://www.wintun.net/) (downloaded at service install, pinned by hash).
