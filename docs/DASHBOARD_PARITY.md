# Dashboard Parity Checklist

Source reference: `../clash-rs/clash-dashboard`.

This checklist tracks the web dashboard surface that the Windows Reactor GUI should cover, plus desktop-only controls needed to make the app shippable.

## Existing Reactor GUI Coverage

- App shell opens as a native Reactor window.
- Snapshot summary loads app settings, active profile, and active core metadata.
- Start, Stop, and Restart actions launch and stop a direct, GUI-owned core session.
- TUN/service-required launches are surfaced as needing the Windows service instead of pretending to start.
- Profile management uses the real manifest store: list profiles, import local YAML files, set active profile, delete managed profiles, ensure the default profile, and validate the active profile with `clash-rs --test-config`.
- Settings controls persist through `SettingsStore`: stable/preview update channel, auto-check, auto-download, TUN launch policy, and system proxy target.
- System proxy actions use the real Windows proxy backend with a saved restore snapshot.
- Status action probes `/version` through the configured named-pipe controller endpoint.
- Update actions check and apply the configured `clash-rs.exe` update channel.
- Architecture selection, global override editing, and profile override editing are exposed in the GUI.
- Windows service install, uninstall, start, stop, status, and service-owned core stop are exposed in the GUI.
- Typed controller helpers exist for configs, proxies, proxy providers, rule providers, rules, connections, memory, DNS query, and restart.
- Controller mutation helpers and GUI controls exist for config patch/reload, proxy selection, delay checks, provider updates, provider health checks, rule-provider rules/match, individual connection close, and close-all-connections.
- The GUI has typed overview, native summary views for proxies, providers, rules, and connections, dashboard inventory, DNS query, and connection cleanup actions.
- Named-pipe WebSocket sampling exists for traffic, connections, logs, and flows.
- Native stream-backed Logs and Flows pages now keep bounded in-memory buffers with filters/toggles/status badges instead of one-shot sample buttons.
- Logs, Flows, and Connections now auto-refresh while their pages are open using native Reactor effects + dispatcher timers.
- Overview now shows live stream refresh controls and shell output feedback with busy/idle state.
- Config reload now includes a native file picker UX in addition to manual path entry.
- Provider inventory now merges proxy and rule providers into a unified table with scope, behavior, format, and count metadata.
- Connections now expose additional parity fields (ASN, process, duration) and include ASN-aware filtering.
- Rules page now supports choosing rule providers directly from the provider inventory surface instead of relying on free-text names.
- `--dev` roots the GUI at workspace `.dev`; production roots at ProgramData.
- Reactor layout is split into `views/*` modules, with `app.rs` reduced to state composition and section assembly.
- Native navigation now mirrors the sibling web dashboard route model: Overview, Proxies, Providers, Rules, Connections, Flows, DNS, Logs, Profiles, Settings, and Service.
- Overview owns controller summary, memory, API restart, config patch/reload, and stream snapshot actions instead of exposing a flat debug toolbar.
- Proxies, Providers, Rules, Connections, DNS, Logs, and Flows have focused native pages with their own inputs and actions.
- Overview, Proxies, Providers, Rules, Connections, and DNS render typed native cards/tables in-page instead of relying on raw text dumps.
- Fixed-choice fields such as mode, log level, and DNS record type use native combo boxes.
- The native app shell now uses a persistent Windows NavigationView, page headers, grouped workflow panels, and compact inline activity feedback instead of a prototype Pivot-and-console layout. Core lifecycle/update commands live on Overview rather than repeating across every page.
- Settings uses radio buttons and toggle switches for update channel, architecture, automatic updates, and TUN policy. Profiles uses dedicated list/import/override panels.
- Profiles can import local YAML, download subscription YAML from a remote URL, refresh the selected profile source, and manage common per-profile overrides through GUI controls with an advanced YAML editor still available.

## Web Dashboard Feature Surface

The dashboard routes are defined in `src/App.tsx` and `src/components/Layout.tsx`:

- Overview
- Flows
- Connections
- Proxies
- Providers
- Rules
- DNS
- Logs
- Settings

The dashboard API layer in `src/lib/api.ts` covers:

- `GET /version`
- `GET /configs`
- `PUT /configs`
- `PATCH /configs`
- `GET /proxies`
- `PUT /proxies/{group}`
- `GET /proxies/{name}/delay`
- `GET /group/{name}`
- `GET /group/{name}/delay`
- `GET /providers/proxies`
- `PUT /providers/proxies/{name}`
- `PUT /providers/proxies/{name}/healthcheck`
- `GET /providers/proxies/{provider}/{proxy}/healthcheck`
- `GET /providers/rules`
- `PUT /providers/rules/{name}`
- `GET /providers/rules/{name}/rules`
- `GET /providers/rules/{name}/match`
- `GET /rules`
- `GET /connections`
- `DELETE /connections`
- `DELETE /connections/{id}`
- `GET /dns/query`
- `GET /memory`
- `POST /restart`
- `WS /ws/traffic`
- `WS /ws/connections`
- `WS /ws/logs`
- `WS /ws/flows`

## Native GUI Parity Targets

### Overview

- Show connection status and core version.
- Show live upload/download speeds from `WS /ws/traffic`.
- Show active connection count and total upload/download from `WS /ws/connections`.
- Show memory in use and OS limit from `/memory`.
- Show editable runtime config controls for mode, log level, ports, bind address, IPv6, and allow-lan through `/configs` PATCH. Controls are placed on the native Overview page.
- Support config reload through `/configs` PUT. File picker UX is available.
- Embed proxy group selection in rule/global modes. Basic selection action exists; polished grouped table UX remains.

### Proxies and Providers

- List proxy groups and current selection.
- Native summary view exists for proxy groups and current selections.
- Select group member through `PUT /proxies/{group}`. Basic action exists.
- Test group delay through `/group/{name}/delay`. Basic action exists.
- List static proxies with type and latency.
- Test static proxy delay through `/proxies/{name}/delay`. Basic action exists.
- List proxy providers with vehicle type, update time, and proxy count.
- Native summary view exists for proxy and rule providers.
- Update proxy providers. Basic action exists.
- Run provider health checks. Basic action exists.
- Test individual provider proxy latency. Basic action exists.

### Rules and Rule Providers

- List active rules with type, payload, and target proxy.
- Native summary view exists for active rules.
- Filter rules by type, payload, and proxy.
- List rule providers with type, vehicle type, behavior, format, rule count, and updated time.
- Update rule providers. Basic action exists.
- Expand classical providers to inspect provider rules. Basic action exists.
- Test provider match for domain/IP targets. Basic action exists.

### Connections

- Stream active connections from `WS /ws/connections`.
- Show host, ASN, network, rule, chain, upload, download, duration, process where available.
- Native summary view exists for connection host, rule, chain, upload, and download.
- Filter connections by host, rule, chain, and ASN.
- Close one connection. Basic action exists.
- Close all connections. Basic action exists.

### Flows

- Stream flow records from `WS /ws/flows?interval=5&top=50&include_closed=...`.
- Toggle inclusion of closed flows.
- Show totals, top destination, active/closed counts, upload/download.
- Filter by destination and protocol.
- Sort by bytes, upload, download, and connection count.
- Render a flow graph equivalent to the dashboard Sankey view or a native table-first replacement if graph rendering is unavailable.
- Render a flow graph equivalent to the dashboard Sankey view or a native table-first replacement if graph rendering is unavailable. Native table-first graph panel is now present.

### DNS

- Query `/dns/query` by hostname and type.
- Support A, AAAA, CNAME, MX, TXT, NS, and SOA.
- Show answer records, TTL, record type, raw status flags, and error states.

### Logs

- Stream logs from `WS /ws/logs`.
- Show websocket connection state.
- Filter by all/debug/info/warning/error.
- Pause/resume autoscroll.
- Clear local log buffer.
- Retain a bounded recent log history.

### Dashboard Settings

- Configure controller API URL and secret for remote-dashboard parity when needed.
- Configure controller API URL and secret for remote-dashboard parity when needed. Controller endpoint + secret fields are now available in Settings and are used by status/controller actions.
- For the desktop app, prefer the managed named-pipe controller endpoint for the bundled core.

## Desktop-Only GUI Targets

These are outside the web dashboard but required for the final Windows app:

- Persist richer running/stopped/error lifecycle state in the Reactor app.
- Display direct-launch vs service-required launch mode in the main layout.
- Route TUN/elevated launches through the installed service.
- Package the native app with installer-grade ProgramData layout, service registration, uninstall UX, and update-safe binary paths. Portable scripts now register service and initialize ProgramData; a full installer remains.

## Recommended Implementation Order

1. Build Overview with live traffic, memory, mode/log/port config controls, and proxy group selection.
2. Build Proxies/Providers and Rules pages on top of the typed controller helpers.
3. Build Connections, Logs, DNS, and Flows pages.
4. Replace stream sampling buttons with persistent native live views for traffic, connections, logs, and flows.
5. Finish installer-grade service/elevated TUN flow and packaging.
