# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`clash-rs-desktop` ships **Choclite**, a minimal desktop client for `clash-rs` ("one switch, it just works"). Tauri 2 shell + SvelteKit (Svelte 5) frontend over a Rust core crate, plus a privileged service for TUN. Windows and Linux. macOS is covered by the native Choclite app, not this repo.

`clash-rs` itself is a downloaded sidecar binary — do not depend on or modify `clash-rs`/`clash-lib` from here unless a feature genuinely needs core support.

## Commands

```powershell
cargo test --workspace                      # Rust tests (core, service, app)
cd app; pnpm install; pnpm check            # frontend type-check
cd app; pnpm tauri dev                      # run the app (hot reload)
cd app; pnpm bundle:windows                 # NSIS installer (bundles the service exe)
cd app; pnpm bundle:linux                   # deb + AppImage (run on Linux)
cargo run -p clash-rs-desktop-core --example engine_smoke -- <dir> <sub-url> [system-proxy|tun]
```

- `CLASH_RS_DESKTOP_CORE=C:\path\to\clash-rs.exe` uses a local core instead of downloading the latest release.
- For TUN in dev, build the service first (`cargo build -p clash-rs-desktop-service`); the app looks for it next to its own exe.
- To drive the dev UI headlessly, launch with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and use CDP; don't send synthetic mouse input to the user's desktop.

## Layout

- `crates/clash-rs-desktop-core` — no UI dependency.
  - `engine`: **the** orchestration seam (`connect`, `disconnect`, `reload`, `check_health`, `install_tun_support`, core updates). Backends: a local child process (system-proxy mode) or the privileged service (TUN).
  - `generate`: builds the running config from layers — TUN defaults → subscription → `override.yaml` → profile override → L2 custom rules (`prepend-rules`, targeting the first `select` group) → shell settings (port, mode, TUN switch, controller endpoint).
  - `config_layers`: YAML deep merge + `prepend-`/`append-` splicing for `rules`/`proxies`/`proxy-groups`.
  - `service`: protocol (`Request`/`Response`, line-delimited JSON), `ServiceClient`, `Supervisor`, `serve`, `sanitize_config`. Transport: named pipe (Windows) / unix socket `/run/choclite/service.sock` (Linux).
  - `process`: spawns the core in a kill-on-close Job Object (Windows) / with `PR_SET_PDEATHSIG` (Linux); drains output into `CoreLog` (also used to detect TUN failures).
  - `ipc`: controller REST/WebSocket client over named pipe / unix socket.
  - `profile`: profiles + subscription download (`subscription-userinfo`, `profile-title`, `profile-update-interval`).
  - `system_proxy`: WinINet via the registry API / GNOME `gsettings`.
  - `elevate`: one UAC prompt (`runas`) / `pkexec`.
  - `binary_update`: GitHub release download (prefers static-CRT / musl builds).
- `crates/clash-rs-desktop-service` — the privileged service binary: `--install [--core]`, `--uninstall`, `--service`, `--console`. Windows: SCM service `ChocliteService`, files in `%ProgramFiles%\Choclite\Service` + `%ProgramData%\Choclite\Service` (ACL-locked), wintun pinned by SHA-256. Linux: systemd unit `choclite.service`, `/opt/choclite/service`, `/var/lib/choclite/service`.
- `app/src-tauri` — Tauri backend: `commands.rs` (flows, commands, traffic stream, watchdog, subscription auto-refresh, deep links), `state.rs` (`View`), `tray.rs`, `nodes.rs`.
- `app/src` — UI. `lib/store.svelte.ts` state; `lib/screens/*` pages; `lib/components/*`; tokens in `app.css` (light/dark, brand periwinkle from the iOS icon).

## Key Invariants

- **Never leave the user's proxy pointing at a dead port.** `Engine` snapshots the OS proxy to `runtime/system-proxy-snapshot.json` before changing it and restores/deletes it on disconnect, on core death (watchdog), on quit, and on the next launch. Keep the oldest snapshot.
- **The service never runs caller-supplied executables or paths.** It launches only its own installed core and receives config *content*, which `sanitize_config` strips of controller/UI keys, pins provider paths into its data dir, and rejects absolute/parent paths in file-path keys.
- **A TUN connection only counts once the device is up.** clash-rs keeps running when TUN fails, so `CoreLog::wait_for_tun` checks the log (success must hold for a settle period, since routes are installed after the success marker).
- **Config changes hot-reload** (`Engine::reload` → `PUT /configs` with `{}`), validated first with `clash-rs --test-config` on a staging file; a rejected config never replaces a working one. Reload is time-bounded; the shell falls back to reconnecting.
- **The UI never waits on the engine lock.** Commands mutate under `AppState.engine`; the UI reads `View`, republished via the `view` event. The watchdog uses `try_lock`.
- **Never claim `clash://` from another client.** `choclite://` is always registered; `clash://` only when unclaimed. Deep-link imports always ask the user to confirm.
- **User-facing errors are Chinese, short and actionable.** Raw logs go to the logs screen.
- **Simple by default**: L0 home (switch, node, subscription); L1 settings; L2 site rules; L3 custom YAML + logs.
