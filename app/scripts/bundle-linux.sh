#!/usr/bin/env bash
# Build deb + AppImage on Linux. Needs the Tauri system packages and
# `cargo install tauri-cli --version ^2`.
set -euo pipefail
cd "$(dirname "$0")/.."

pnpm build
cargo build --release -p clash-rs-desktop-service
target_dir="${CARGO_TARGET_DIR:-../target}"
mkdir -p src-tauri/linux/bin
install -m 755 "$target_dir/release/clash-rs-desktop-service" src-tauri/linux/bin/

cd src-tauri
cargo tauri build --bundles deb,appimage --config tauri.bundle-linux.json "$@"
