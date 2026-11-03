# Implementation Plan

Goal: a desktop client a non-technical user can run with one switch, with advanced options one level down (L0 → L3), that still works for people with intranets, NAS boxes, several providers or self-hosted servers.

## Done

- Tauri 2 + Svelte 5 shell; ChocLite branding; light/dark/system theme.
- `Engine` with two backends (local child process, privileged service); crash-safe system proxy snapshot/restore; watchdog.
- Privileged service for TUN (Windows SCM + named pipe, Linux systemd + unix socket): runs only its own core, sanitises configs, versioned (`+svcN`) with one-prompt updates, adopts a running tunnel across versions, flushes OS DNS cache, clears stale Linux policy routes.
- TUN health: connected only once the device is up and DNS answers; optional IPv6 capture.
- Subscriptions: Clash YAML, base64 link lists, share links, local files; userinfo/title/update-interval; background refresh; merging several subscriptions; rollback when a config can't run.
- Config generation: L2 site rules, bypass list + automatic local networks (system DNS via `nameserver-policy`, no fake IPs, WinINet exceptions), overlays with list splicing, hot reload.
- Node picks remembered per subscription across restarts and mode switches.
- Network diagnostics with one-click fixes and a quiet background health check; proxy-bypass hint (TCP table).
- Deep links (`choclite://`, `clash://` when unclaimed) with confirmation; tray; launch at login; auto-connect; core updates; rotating log files.
- Packaging: NSIS (with service + uninstall hook), deb (prerm removes service), AppImage.

## Verified on real machines

- Windows: onboarding, first-run core download, system proxy connect/restore, crash recovery, TUN install via UAC, service update via UAC, TUN connect with immediate connectivity, internal domain over TUN resolved to its LAN address, bypass list, site-rule hot reload, broken-config rollback, merging, node memory across modes, leak hint (real launchers detected with friendly names), diagnostics (proxy hijack + fix, IPv6 capture routes), installer install/uninstall.
- Linux (WSL Debian): tests, build, system-proxy flow, TUN via systemd service, deb install/remove.

## Not yet verified

- Linux `pkexec` prompt in a real desktop session (WSL has no polkit agent; tested with `sudo`).
- IPv6 capture end to end on a network with working IPv6 (routes verified; the test network has none).

## Known issues

- Right after toggling IPv6 capture, the first request can fail while IPv6 routes settle.

## Next

1. Code signing (Windows Authenticode) — reduces SmartScreen/AV friction for the installer and service.
2. App self-update (Tauri updater) next to the existing core updater.
3. KDE proxy backend (`kwriteconfig`) on Linux; leak hint on Linux.
4. Decide whether IPv6 capture should default on once verified on IPv6 networks.
