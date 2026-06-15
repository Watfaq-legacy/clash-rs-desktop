# clash-rs-desktop

Native Windows shell for `clash-rs`.

This workspace is organized around three boundaries:

- `clash-rs-desktop-core`: shared app state, config layering, update metadata, process contracts, and named-pipe API types.
- `clash-rs-desktop-service`: privileged Windows service/supervisor for elevated core lifecycle, especially TUN.
- `clash-rs-desktop-ui`: native Windows UI shell intended to use `windows-reactor`.

The UI crate is split by responsibility: `app.rs` owns state composition and delegates layout to `views/*`, `runtime_actions.rs` owns core lifecycle, `profile_actions.rs` owns profile management, `settings_actions.rs` owns persisted settings and system proxy operations, `service_actions.rs` owns service pipe commands, `controller_actions.rs` owns Clash controller API calls, and `paths.rs` owns app roots and pipe paths. View modules are grouped by surface: dashboard/controller workflows, profiles, settings, service controls, and the shared design system.

The desktop app talks to the Clash controller over a Windows named pipe, using the same controller surface as the existing `clash-dashboard` HTTP/WebSocket dashboard.

See `docs/IMPLEMENTATION_PLAN.md` for the full build-out plan and execution order.

## Runtime Model

The shell generates a runtime config from layered sources:

1. Imported profile config.
2. Global overrides.
3. Per-profile overrides.
4. Shell-required runtime overrides such as `external-controller-pipe`.

The service owns the active `clash-rs.exe` process when privileged features are needed. The UI remains non-elevated.

## Toolchain

Use Rust 1.95 or newer. The optional `windows-reactor` UI feature requires that minimum compiler version because the upstream `windows-reactor` crate declares it.
The local Cargo config sets `CARGO_WORKSPACE_DIR` because the upstream `windows-reactor` build script expects that variable when staging Windows App SDK assets. It points at `target/windows-reactor` so generated `temp` and `winmd` folders do not appear in the workspace root.

```powershell
cargo check --workspace --all-targets
cargo test --workspace
cargo check -p clash-rs-desktop-ui --features reactor
```

The shipped UI is the Reactor GUI build:

```powershell
cargo run -p clash-rs-desktop-ui --features reactor
```

Use `--dev` to keep all runtime data under workspace `.dev` (default root is `%PROGRAMDATA%/clash-rs-desktop`):

```powershell
cargo run -p clash-rs-desktop-ui --features reactor -- --dev
```

The Reactor dashboard includes actions for start, stop, restart, refresh, status probe, real profile management backed by the manifest store, global/profile YAML overrides, settings persistence, Windows system proxy apply/restore, typed overview, native proxy/provider/rule/connection summaries, controller config patch/reload, proxy selection and delay tests, provider updates and health checks, rule-provider inspection/match tests, DNS query, dashboard inventory, connection cleanup, live stream sampling, and Windows service install/start/stop/uninstall commands.

Preview channel note: upstream uses a constant `latest` tag, so update detection cannot rely on tag changes alone. The updater compares selected asset/version metadata to detect preview updates.

The smoke example creates an active default profile under `.dev/profiles`, validates the merged runtime config with `clash-rs --test-config`, starts the core with a unique named pipe, calls `/version`, and stops the process. The GUI creates/loads persistent app settings at `.dev/settings.json` and can import, select, delete, and validate profiles under `.dev/profiles` when run with `--dev`.

The same flow is available through `CoreRuntimeManager`, which is the backend seam intended for the UI and service.

VS Code tasks are available for:

- `cargo: check workspace`
- `cargo: test workspace`
- `clash-rs desktop: run reactor UI dev root`
- `clash-rs desktop: package portable`
- `clash-rs desktop: check reactor UI`

## Current Backend Features

- Active core metadata loading/saving, including SHA-256 metadata and RFC 3339 timestamps.
- GitHub release metadata checking, Windows asset selection (direct `.exe` or ZIP), SHA-256 activation metadata, preview-channel non-tag update detection, and rollback binary preservation.
- Persistent app settings for update channel, auto-update preferences, system proxy target, and TUN launch policy.
- Profile manifest storage, default profile creation, local profile import, active profile switching.
- Runtime config generation from active profile, optional global override, optional profile override, and shell-required named-pipe override.
- Runtime config validation through `clash-rs --test-config` before direct launch.
- TUN detection from merged config; `tun.enable: true` returns a service-required state instead of launching directly.
- Named-pipe `/version` probing over the Clash controller API.
- Typed named-pipe controller helpers for key dashboard REST endpoints, including configs, proxies, providers, rules, connections, memory, DNS query, and restart.
- Controller mutation helpers for config patch/reload, proxy selection, delay checks, provider updates, provider health checks, rule-provider inspection/match checks, and individual/all connection close.
- Named-pipe WebSocket sampling for traffic, connections, logs, and flows streams.
- Service command shell using line-delimited JSON over stdin for development and a Windows named pipe for production-style UI/service transport.
- Windows Service Control Manager integration for install, uninstall, start, stop, status commands, and cooperative SCM shutdown.
- Windows system proxy backend primitives for snapshot/apply/restore.

## Service Pipe Transport

Install and start the service from an elevated shell:

```powershell
.\target\release\clash-rs-desktop-service.exe --install
.\target\release\clash-rs-desktop-service.exe --start-service
```

The packaged `Install-Service.ps1` script performs the same registration and initializes `%PROGRAMDATA%\clash-rs-desktop` folders for `profiles`, `runtime`, and `core`.

Check or stop the installed service:

```powershell
.\target\release\clash-rs-desktop-service.exe --service-status
.\target\release\clash-rs-desktop-service.exe --stop-service
```

Uninstall the service:

```powershell
.\target\release\clash-rs-desktop-service.exe --uninstall
```

For development without SCM registration, run the service named-pipe host directly:


```powershell
cargo run -p clash-rs-desktop-service -- --pipe
```

In another terminal, after the service binary has been built, request status through the pipe:

```powershell
.\target\debug\clash-rs-desktop-service.exe --pipe-status
```

Expected response:

```json
{"status":"state","state":"stopped"}
```

## Update Channels

- Stable: GitHub `/releases/latest`.
- Preview: GitHub `/releases/tags/latest`.

Only the selected `clash-rs.exe` is kept, plus one optional rollback copy. `wintun.dll` is managed once for the current architecture, not per Clash version.
