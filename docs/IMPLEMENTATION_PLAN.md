# Implementation Plan

This plan turns the current scaffold into a usable Windows desktop shell for `clash-rs`. Work is split into vertical slices so every phase leaves the app buildable and testable.

## Phase 1: Core Runtime Orchestration

Goal: make the non-UI core crate own a complete local launch flow.

- Add an app-level orchestrator that loads active binary metadata, generates runtime config, decides direct vs service launch, starts/stops the core, and probes `/version` over the named pipe.
- Keep all runtime paths under an `AppPaths` root so production can use ProgramData while local dev can use `.dev`.
- Preserve the layered config model: base profile, global override, profile override, shell override.
- Add health/status output that the UI and service can serialize.
- Keep direct launch for non-elevated mode and return `NeedsService` when TUN or route mutation requires elevation.

## Phase 2: Profile and Config Management

Goal: support user config profiles without mutating imported configs.

- Add a profile store manifest with active profile selection.
- Support local file imports, remote URL metadata, and generated inline templates.
- Store profile source files, profile overrides, and generated runtime config separately.
- Add profile CRUD operations and active-profile switching.
- Validate generated YAML before launch with `clash-rs --test-config`.

## Phase 3: System Proxy and TUN Control

Goal: expose safe system-level toggles with restore behavior.

- Model system proxy enable/disable/apply/restore operations.
- Snapshot existing proxy state before changing Windows settings.
- Detect whether TUN settings require service-owned elevated launch.
- Keep Wintun packaging separate from Clash binary updates.
- Make shutdown restore proxy state and stop elevated core reliably.

## Phase 4: Service Boundary

Goal: implement a privileged supervisor that the UI can command over a named pipe.

- Implement service pipe transport using line-delimited JSON commands and responses.
- Add `StartCore`, `StopCore`, `RestartCore`, `Status`, `ApplyUpdate`, and `RollbackUpdate` handlers.
- Ensure child process cleanup on service exit.
- Add request validation so the service only launches managed binaries/config paths.
- Later package as a real Windows Service with install/uninstall commands.

## Phase 5: Controller API Coverage

Goal: match the dashboard-backed operational surface.

- Use `docs/DASHBOARD_PARITY.md` as the source checklist for the GUI-facing API and page surface from `../clash-rs/clash-dashboard`.
- Keep typed wrappers for `/version`, `/configs`, `/proxies`, `/providers/*`, `/rules`, `/connections`, `/dns/query`, `/memory`, and restart.
- Add WebSocket clients for logs, traffic, and flows.
- Add tests for endpoint paths and request serialization.

## Phase 6: Native UI

Goal: build a Reactor UI around the core orchestration.

- Create pages for overview, profiles, proxies/providers, rules/connections, logs, settings, and updates.
- Start with a read-only shell that displays runtime state and version.
- Add start/stop/restart actions and profile switching.
- Add settings for update channel, system proxy, TUN, and global overrides.
- Keep UI non-elevated; use the service for privileged operations.

## Phase 7: Updates and Packaging

Goal: safely manage `clash-rs.exe` updates and installer lifecycle.

- Implement stable/latest-preview GitHub release lookup.
- Download architecture-specific assets, verify checksums when available, and write active metadata.
- Keep one rollback binary.
- Add app packaging, service install, and code-signing hooks.

## Current Execution Slice

Completed backend slices:

- Phase 1 core runtime orchestration.
- Phase 2 profile manifest foundations, default profile creation, local profile import, active profile launch, and `clash-rs --test-config` validation.
- Phase 3 persistent app settings, initial TUN requirement detection, and Windows system proxy backend primitives.
- Phase 4 service command shell with stdin development mode and Windows named-pipe transport.
- Phase 6 initial Reactor GUI shell with refresh/status/update actions, direct Start/Stop/Restart lifecycle controls, real profile list/import/select/delete/default/validate actions, settings persistence controls, and Windows system proxy apply/restore.
- Phase 7 release metadata checking, Windows asset selection, ZIP extraction, SHA-256 activation metadata, and rollback preservation.

Next execution slice: expand the Reactor UI with architecture selection, profile/global override editing, service-backed elevated launch, and the dashboard parity pages tracked in `docs/DASHBOARD_PARITY.md`.

## GUI Gap Closure Todo Backlog (May 2026)

This backlog converts the current GUI parity gaps into execution-ready todos. Keep this section updated as work lands.

### Priority P0 (Ship Blockers)

- [ ] Restore visible action output in the app shell so every command has user-facing success/error feedback.
- [ ] Add lightweight busy/disabled states for long-running actions (stream start/stop, provider refresh, close-all connections).
- [ ] Replace one-shot stream sampling with persistent live stream infrastructure (traffic, connections, logs, flows) with reconnect and bounded buffering.
- [ ] Remove Overview "Controller samples" actions and replace with live widgets backed by persistent streams.
- [ ] Implement a full Flows page: include-closed toggle, totals, top destination, active/closed counts, filter (destination/protocol), and sort (bytes/upload/download/connection count).
- [ ] Implement a full Logs page: connection status, level filter, pause/resume autoscroll, clear buffer, and bounded history.
- [ ] Add tests for stream lifecycle and buffering behavior (initial connect, reconnect, teardown, cap eviction).

### Priority P1 (High Value Parity)

- [ ] Enrich Connections model and table with parity fields: ASN, process, and duration, with graceful fallbacks when unavailable.
- [ ] Extend connection filtering to include ASN (and process if present).
- [ ] Split provider surfaces into explicit Proxy Provider and Rule Provider inventory views.
- [ ] Add Rule Provider inventory metadata columns: type, vehicle type, behavior, format, rule count, updated time.
- [ ] Wire rule-provider actions (update, inspect rules, match test) from selected inventory rows.
- [ ] Replace config reload path text input with a file picker flow (keep manual path fallback optional).

### Priority P2 (Product Decision / Nice-to-Have)

- [ ] Decide settings direction for remote dashboard parity:
	- Desktop-only managed named-pipe endpoint, or
	- Optional remote controller URL + secret.
- [ ] If remote mode is approved, add settings UI + persistence + runtime client wiring for endpoint/secret.
- [ ] Optional: add a chart panel for Flows after table-first parity is complete.

### Validation and Release Gate

- [ ] Run `cargo check --workspace --all-targets`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo check -p clash-rs-desktop-ui --features reactor`.
- [ ] Update `docs/DASHBOARD_PARITY.md` status lines to reflect delivered GUI parity.
- [ ] Capture known non-goals and deferred work explicitly in docs before merge.
