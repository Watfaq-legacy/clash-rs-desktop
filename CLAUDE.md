# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`clash-rs-desktop` ships **Choclite**, a minimal desktop client for `clash-rs` ("one switch, it just works"). Tauri 2 shell + SvelteKit (Svelte 5) frontend over a Rust core crate. Windows first; Linux next. macOS is covered by the native Choclite app, not this repo.

`clash-rs` itself is a downloaded sidecar binary — do not depend on or modify `clash-rs`/`clash-lib` from here unless a feature genuinely needs core support.

## Commands

```powershell
cargo test --workspace                      # Rust tests (core, service, app)
cd app; pnpm install; pnpm check            # frontend type-check
cd app; pnpm tauri dev                      # run the app (hot reload)
cd app; pnpm tauri build                    # NSIS installer
```

Set `CLASH_RS_DESKTOP_CORE=C:\path\to\clash-rs.exe` to use a local core instead of downloading the latest release on first connect.

## Layout

- `crates/clash-rs-desktop-core` — no UI dependency.
  - `engine`: **the** orchestration seam. `Engine::connect/disconnect/check_health`; owns the core process, generated config and OS proxy. All shells go through it.
  - `config_layers`: layered YAML merge. Order: subscription → global override (`override.yaml`) → profile override → shell settings (port, mode, `tun.enable=false`, controller pipe). Overlays support `prepend-`/`append-` + `rules`/`proxies`/`proxy-groups`; named entries replace same-named ones; appended rules land before `MATCH`.
  - `process`: spawns `clash-rs.exe` inside a kill-on-close **Job Object** and drains stdout/stderr into a bounded `CoreLog`.
  - `profile`: profile manifest + subscription download (`fetch_subscription`, `subscription-userinfo`, `profile-title`).
  - `system_proxy`: WinINet proxy via the registry API (+ `InternetSetOption` refresh).
  - `ipc`: controller REST/WebSocket client over a named pipe.
  - `binary_update`: GitHub release download of `clash-rs.exe`.
  - `service`: privileged Windows service protocol — **legacy, not used by the app yet** (see plan).
- `crates/clash-rs-desktop-service` — SCM service binary (legacy, pending rewrite for TUN).
- `app/src-tauri` — Tauri backend: `commands.rs` (connect flow, commands, traffic stream, health watchdog), `state.rs` (`View` snapshot), `tray.rs`, `nodes.rs` (main-group/node list derivation).
- `app/src` — UI. `lib/store.svelte.ts` holds state; `lib/screens/*` are pages; `lib/components/*` shared widgets; design tokens in `app.css`.

## Key Invariants

- **Never leave the user's proxy pointing at a dead port.** `Engine` writes `runtime/system-proxy-snapshot.json` before changing the OS proxy and deletes it after restoring. On startup a leftover snapshot is restored. Keep the oldest snapshot; never overwrite an existing one.
- **Disconnect restores the network before stopping the core.**
- **The UI never waits on the engine lock.** Commands mutate under `AppState.engine` (async mutex, may be held for a download); the UI reads `View`, republished via the `view` event after every change. The watchdog uses `try_lock`.
- **Errors shown to users are Chinese, short and actionable.** Use `anyhow::Context` with user-facing text at the boundary; raw log lines go to the logs screen, not toasts.
- **Simple by default**: the home screen is L0 (switch, node, subscription). Clash concepts (rules, groups, YAML) only appear under Settings → 高级.
- Data root is the per-user app-local data dir (`%LOCALAPPDATA%\com.choclite.desktop`).
