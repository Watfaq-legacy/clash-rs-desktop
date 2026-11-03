# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`clash-rs-desktop` ships **Choclite**, a minimal desktop client for `clash-rs` ("one switch, it just works"). Tauri 2 shell + SvelteKit (Svelte 5) frontend over a Rust core crate, plus a privileged service for TUN. Windows and Linux. macOS is covered by the native Choclite app, not this repo.

`clash-rs` itself is a downloaded sidecar binary — do not depend on or modify `clash-rs`/`clash-lib` from here unless a feature genuinely needs core support.

## Commands

```powershell
cargo test --workspace                      # Rust tests (core, service, app)
cargo clippy --workspace --all-targets      # keep at zero warnings
cargo fmt --all
cd app; pnpm install; pnpm check            # frontend type-check
cd app; pnpm tauri dev                      # run the app (hot reload)
cd app; pnpm bundle:windows                 # NSIS installer (bundles the service exe)
cd app; pnpm bundle:linux                   # deb + AppImage (run on Linux)
cargo run -p clash-rs-desktop-core --example engine_smoke -- <dir> <sub-url> [system-proxy|tun]
```

- `CLASH_RS_DESKTOP_CORE=C:\path\to\clash-rs.exe` uses a local core instead of downloading the latest release.
- For TUN in dev, build the service first (`cargo build -p clash-rs-desktop-service`); the app looks for it next to its own exe.
- Drive the dev UI headlessly via CDP: launch with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`. Don't send synthetic mouse input, and don't script proxy-registry edits from hidden PowerShell — Defender flags that as `Trojan:Win32/ClickFix`.

## Layout

- `crates/clash-rs-desktop-core` — no UI dependency.
  - `engine`: **the** orchestration seam (`connect`, `disconnect`, `reload`, `check_health`, `diagnose`, `apply_fix`, `install_tun_support`, core updates, node-pick memory). Backends: a local child process (system-proxy mode) or the privileged service (TUN).
  - `generate`: builds the running config from layers — TUN defaults → subscription (+ merged subscriptions) → `override.yaml` → profile override → bypass/local rules → L2 site rules → app settings layer (port, mode, TUN switch, IPv6 capture, controller endpoint, `store-selected`).
  - `config_layers`: YAML deep merge + `prepend-`/`append-` splicing for `rules`/`proxies`/`proxy-groups`.
  - `network`: the machine's DNS search suffixes and system DNS servers (registry / resolv.conf).
  - `diagnose`: read-only checks with pure verdicts (`assess`) and per-platform collection (IP Helper APIs on Windows, `ip` on Linux — never PowerShell).
  - `leak`: programs connecting around the system proxy (TCP table + version-info names).
  - `links`: share links (`ss://`, `trojan://`, `vmess://`, `vless://`, `hysteria2://`) and base64 link lists → Clash config.
  - `service`: protocol (`Request`/`Response`, line-delimited JSON), `ServiceClient`, `Supervisor`, `serve`, `sanitize_config`. Transport: named pipe (Windows) / unix socket `/run/choclite/service.sock` (Linux).
  - `process`: spawns the core in a kill-on-close Job Object (Windows) / with `PR_SET_PDEATHSIG` (Linux); `CoreLog` keeps a tail in memory, mirrors to rotating `logs/core.log`, and detects TUN failures.
  - `profile`: profiles, subscription download (`subscription-userinfo`, `profile-title`, `profile-update-interval`), file/link import, merge selection.
  - `ipc`, `system_proxy`, `elevate`, `binary_update`, `settings`, `paths`.
- `crates/clash-rs-desktop-service` — privileged service binary: `--install [--core]`, `--uninstall`, `--service`, `--console`. Windows: SCM service `ChocliteService`, `%ProgramFiles%\Choclite\Service` + `%ProgramData%\Choclite\Service` (ACL-locked), wintun pinned by SHA-256. Linux: systemd unit `choclite.service`, `/opt/choclite/service`, `/var/lib/choclite/service`.
- `app/src-tauri` — Tauri backend: `commands.rs` (flows, commands, background tasks: traffic, watchdog, health check, leak watch, subscription refresh, deep links), `state.rs` (`View`), `tray.rs`, `nodes.rs`.
- `app/src` — UI. `lib/store.svelte.ts` state + navigation; `lib/screens/*`; `lib/components/*`; tokens in `app.css` (light/dark, brand periwinkle).

## Key Invariants

- **Never leave the user's proxy pointing at a dead port.** `Engine` snapshots the OS proxy to `runtime/system-proxy-snapshot.json` before changing it and restores/deletes it on disconnect, core death, quit, and next launch. Keep the oldest snapshot.
- **The service never runs caller-supplied executables or paths.** It launches only its own installed core and receives config *content*, which `sanitize_config` strips of controller/UI keys, pins provider paths, and rejects absolute/parent paths in file keys.
- **Bump `SERVICE_VERSION`'s `+svcN` suffix whenever service behaviour changes**; the app then asks for one UAC/pkexec prompt to update. A running TUN core is adopted regardless of service version (never show "disconnected" over a live tunnel).
- **A TUN connection only counts once the device is up and DNS answers** (`CoreLog::wait_for_tun`, `wait_dns`); the service flushes the OS DNS cache on start/stop.
- **Config changes hot-reload**, validated with `clash-rs --test-config` on a staging file. Profile changes (switch/import/merge) go through `change_profiles`, which rolls back to the previous selection if the core can't run the result.
- **The UI never waits on the engine lock.** Commands mutate under `AppState.engine`; the UI reads `View`, republished via the `view` event. Background tasks use `try_lock`.
- **App settings override config files**, never the other way round: they live in the top layer of `generate`.
- **Never claim `clash://` from another client.** `choclite://` always; `clash://` only when unclaimed. Imports always ask for confirmation.
- **User-facing text is Chinese, short and actionable.** Hints are quiet (no alarm colours for non-errors); details go in sheets/screens; raw logs go to the log screen/files.
- **Simple by default**: L0 home (switch, node, subscription); L1 settings; L2 site rules / bypass list; L3 custom YAML, logs, diagnostics.
