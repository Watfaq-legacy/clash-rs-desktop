# Implementation Plan

Goal: a desktop client a non-technical user can run with one switch, with advanced options one level down (L0 → L3).

| Level | Who | Surface |
|---|---|---|
| L0 | everyone | power switch, current node, subscription usage |
| L1 | some | routing mode, launch at login, auto-connect, manual node pick |
| L2 | power users | add rules without YAML (planned) |
| L3 | experts | custom config overlay (YAML), logs |

## Done (phase 1 — system proxy mode)

- Tauri 2 + Svelte 5 shell replacing the `windows-reactor` UI.
- `Engine` orchestration: connect/disconnect/watchdog, crash-safe system proxy snapshot/restore.
- Core process in a kill-on-close job object; stdout/stderr drained into a bounded log (fixes pipe-fill stalls and orphaned cores; replaces PID-file kill, which could hit reused PIDs).
- System proxy via the registry API with WinINet refresh (fixes values being lost on restore; browsers pick up changes immediately). LAN ranges bypassed.
- Subscription import/refresh with `subscription-userinfo` / `profile-title`.
- Config overlay `prepend-`/`append-` list splicing.
- Tray (toggle/show/quit), single instance, close-to-tray, launch at login, auto-connect.
- Screens: onboarding, home, nodes (latency test), settings, subscriptions, custom config, logs.

## Next

1. **TUN ("增强模式") via a rewritten service.** The legacy service accepts arbitrary `binary_path`/`config_path` from any authenticated user and runs them as LocalSystem — a local privilege escalation. Rewrite so that:
   - the service only launches the core binary from its own admin-only directory;
   - the UI sends config *content*; the service writes it to an admin-only dir and validates it;
   - the core runs in a job object owned by the service; logs are captured and served over the pipe;
   - requests carry an id and the pipe server handles clients concurrently (a slow start must not block status).
   One UAC prompt to install; the toggle stays one click afterwards.
2. App and tray icons from the Choclite brand; connected/disconnected tray states.
3. `choclite://install-config?url=` deep link for one-click import from provider sites.
4. Auto-refresh subscriptions on the provider's `profile-update-interval`.
5. Core auto-update check (settings → about), using `binary_update`.
6. L2 rule editor ("this site: proxy / direct") writing to the overlay.
7. Hot reload via the controller instead of reconnecting on config changes.
8. Linux: unix-socket controller, gsettings/env proxy backend, systemd/polkit for TUN.
