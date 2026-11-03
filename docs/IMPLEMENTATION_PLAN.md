# Implementation Plan

Goal: a desktop client a non-technical user can run with one switch, with advanced options one level down (L0 → L3).

## Done

- Tauri 2 + Svelte 5 shell replacing the `windows-reactor` UI; ChocLite branding (icon, periwinkle palette), light/dark/system theme.
- `Engine` orchestration with two backends (local child process, privileged service); crash-safe system proxy snapshot/restore; watchdog.
- Core process in a kill-on-close job (Windows) / `PR_SET_PDEATHSIG` (Linux), output drained into a bounded log.
- System proxy via the registry API + WinINet refresh (Windows) and gsettings (Linux).
- Privileged service rewrite for TUN (Windows SCM + named pipe, Linux systemd + unix socket): only runs its own core, sanitises configs, concurrent clients, logs readable while busy, bounded reload, stale-route cleanup on Linux, wintun pinned by hash.
- TUN health: a connection only counts once the TUN device is up (log-based, with settle period).
- Subscription import/refresh, `subscription-userinfo` / `profile-title` / `profile-update-interval`, background auto-refresh.
- Config generation: TUN DNS defaults, L2 site rules, overlays with list splicing; hot reload validated on a staging file.
- Deep links (`choclite://`, `clash://` when unclaimed) with confirmation sheet.
- Core update check/apply (also updates the service's copy).
- Tray with connected/disconnected icons; launch at login; auto-connect; single instance; close-to-tray.

## Verified

- Windows: onboarding, first-run core download (static-CRT asset), connect/disconnect with exact proxy restore, crash recovery (app killed, core killed), node switching, latency test, subscription refresh, deep-link import via second instance, site rules hot reload (core PID unchanged, rule live in `/rules`), invalid custom config rejected without dropping the connection, dark theme, settings.
- Linux (WSL, Debian): core tests, app build, system-proxy flow via unix-socket controller + gsettings, TUN via the systemd service (device + policy routes up, hot reload, clean teardown), TUN failure detection.

## Not yet verified

- Windows TUN end to end (needs a UAC click): service install, wintun download, TUN start/stop, re-adoption after app restart.
- Linux `pkexec` prompt in a real desktop session (WSL has no polkit agent; tested with `sudo`).

## Next

1. Verify Windows TUN with a UAC click; then installer smoke test on a clean VM.
2. Code signing (Windows Authenticode) — also reduces SmartScreen friction for the service install.
3. App self-update (Tauri updater) next to the existing core updater.
4. Base64/v2ray-link subscriptions (currently only Clash YAML).
5. KDE proxy backend (`kwriteconfig`) on Linux.
