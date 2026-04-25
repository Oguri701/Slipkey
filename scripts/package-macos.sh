#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "cargo-tauri is required. Install it with:"
  echo "  cargo install tauri-cli --version '^2'"
  exit 1
fi

cargo test --workspace
cd "$ROOT/bins/imeswitch-app"
# tauri build ad-hoc signs the .app with identity "-" before bundling it
# into the DMG and then deletes target/release/bundle/macos/imeswitch.app,
# so any post-bundle codesign step on that path runs against nothing.
cargo tauri build --bundles dmg
cd "$ROOT"

echo "macOS bundle output:"
find target/release/bundle -maxdepth 3 -name '*.dmg' -print
