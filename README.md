# Choclite for Desktop

A one-switch desktop client for [clash-rs](https://github.com/Watfaq/clash-rs). Paste a subscription, press the button, done. Power-user options live one level down.

Built with Tauri 2 + Svelte 5 over a Rust core. Windows first; Linux planned.

## Develop

Requirements: Rust 1.95+, Node 20+, pnpm.

```powershell
cd app
pnpm install
pnpm tauri dev
```

On first connect the app downloads the latest `clash-rs` release. To use a local build instead:

```powershell
$env:CLASH_RS_DESKTOP_CORE = 'C:\path\to\clash-rs.exe'
```

Tests:

```powershell
cargo test --workspace
cd app; pnpm check
```

## How it works

- **Connect**: ensure core binary → merge config layers → validate with `clash-rs --test-config` → start the core in a kill-on-close job object → wait for the controller → set the system proxy.
- **Config layers**: subscription → your custom config → app-owned settings. Custom config can add rules without replacing the subscription's (`prepend-rules`, `append-rules`, `prepend-proxies`, `prepend-proxy-groups`, ...).
- **Safety**: the original system proxy is snapshotted to disk before it is changed and restored on disconnect, on core crash (watchdog), on quit, and on the next launch after a crash.

See `docs/IMPLEMENTATION_PLAN.md` for what is next.

Flag glyphs: [Twemoji](https://github.com/twitter/twemoji) country flags, CC-BY 4.0.
