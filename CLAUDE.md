# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`clash-rs-desktop` is a native Windows GUI shell for `clash-rs`. It is a Cargo workspace with three crates and requires Rust 1.95+. All commands run from the workspace root.

## Commands

```powershell
# Check everything compiles
cargo check --workspace --all-targets

# Run all tests
cargo test --workspace

# Check UI crate (includes windows-reactor dependency resolution)
cargo check -p clash-rs-desktop-ui --features reactor

# Run UI in dev mode (data stays in .dev/ instead of %PROGRAMDATA%)
cargo run -p clash-rs-desktop-ui --features reactor -- --dev

# Package portable release
powershell -ExecutionPolicy Bypass -File .\scripts\package-portable.ps1
```

## Crate Architecture

**`clash-rs-desktop-core`** — no UI dependency; consumed by both the UI and service.
- `binary_update`: GitHub release fetching, SHA-256 verification, update channel management, rollback.
- `config_layers`: layered YAML merge (`ConfigLayers` → `RuntimeConfigBuilder` → `GeneratedRuntimeConfig`). Layer order: base profile → global override → profile override → shell runtime override (injects `external-controller-pipe`).
- `ipc`: `ClashIpcClient` — typed HTTP/WebSocket client over a Windows named pipe to the Clash controller REST API.
- `process`: `ClashProcessManager` / `CoreLaunchRequest` — spawns and tracks `clash-rs.exe`.
- `runtime`: `CoreRuntimeManager` — the main seam; orchestrates config generation, validation via `--test-config`, and process lifecycle. Also provides `runtime_options_from_active_profile` and `local_dev_runtime_options`.
- `service`: `ServiceCommand` / `ServiceResponse` enums (line-delimited JSON protocol), `ServiceSupervisor`, and `pipe_transport` for Windows named-pipe IPC between the UI and the elevated service.
- `profile`: `ProfileStore`, `ProfileManifest`, `UserProfile` — manifest-backed profile storage.
- `settings`: `AppSettings`, `SettingsStore`, `SystemProxySettings`, `TunLaunchPolicy`.
- `system_proxy`, `tun`, `paths`: Windows system proxy primitives, TUN detection, `AppPaths` layout.

**`clash-rs-desktop-service`** — thin binary over the core library.
- `main.rs`: CLI entrypoints (`--pipe`, `--install`, `--uninstall`, `--start-service`, `--stop-service`, `--service-status`, `--service`) dispatching to stdin host or named-pipe host.
- `scm.rs`: Windows Service Control Manager integration.

**`clash-rs-desktop-ui`** — gated behind the `reactor` feature; does not compile as a useful binary without it.
- `main.rs`: parses `--dev` flag, calls `Application::start()` from `windows-reactor`.
- `app.rs`: root `app()` function — composes all state structs, owns live-refresh timer logic (`use_effect_with_cleanup` + `DispatcherTimer`), builds the `NavigationView`.
- Action modules — each is synchronous, returns `String` for display in the UI:
  - `runtime_actions`: start/stop/restart core, manages a process-global `Mutex<Option<CoreRuntimeManager>>`.
  - `profile_actions`: profile summaries, import, selection, override forms.
  - `settings_actions`: load/save `AppSettings`, global override YAML, controller endpoint config.
  - `service_actions`: send `ServiceCommand` over the service named pipe; SCM operations.
  - `controller_actions`: typed wrappers over `ClashIpcClient` for all dashboard REST endpoints.
- `controller_endpoint.rs`: persists controller pipe path + secret to `controller-endpoint.json`.
- `controller_models.rs`: snapshot types (`ProxySnapshot`, `ConnectionsSnapshot`, etc.) built from `ClashIpcClient` responses.
- `views/`: rendering modules — `dashboard`, `profiles`, `settings`, `service`, `design` (shared widgets: `Toast`, `busy_strip`), `output` (`SafetyState` for two-step destructive actions).

## Key Invariants

**`windows-reactor` is pinned to `rev = "9ed8353d"`** (git dependency from `microsoft/windows-rs`). Commit `7a41196d` breaks `Application::start` with error `0x80040154` — do not advance this pin without testing the app starts.

**`CARGO_WORKSPACE_DIR`** is set to `""` (relative) in `.cargo/config.toml` so the `windows-reactor` build script can find workspace assets; the build writes into `target/windows-reactor` to keep generated `temp/` and `winmd/` folders off the workspace root. Do not remove this env var.

**Dev root vs production root**: `--dev` flag makes `paths.rs` resolve data to `.dev/` (workspace-relative). Without it, paths resolve to `%PROGRAMDATA%\clash-rs-desktop`. The `app_pipe_path()` is always `\\.\pipe\clash-rs-desktop-core-local`; the service pipe is a different constant in `core/src/service.rs`.

**TUN requires the Windows service**: if the merged runtime config has `tun.enable: true`, `launch_requirements_from_config` returns `requires_service = true`. The UI then routes the `StartCore` command over the service named pipe instead of launching `clash-rs.exe` directly. The service binary must be installed and running to accept those commands.

**Controller transport is a Windows named pipe**, not HTTP. `ClashIpcClient` speaks the same REST surface as the web dashboard but over `\\.\pipe\...` using Hyper over a Tokio named-pipe connector.
