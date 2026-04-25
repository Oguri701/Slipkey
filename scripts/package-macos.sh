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
cargo tauri build --bundles dmg
cd "$ROOT"

APP="target/release/bundle/macos/imeswitch.app"
if [ -d "$APP" ]; then
  codesign --force --deep --sign - "$APP"
  codesign --verify --deep --strict "$APP"
fi

echo "macOS bundle output:"
find target/release/bundle -maxdepth 3 \( -name '*.app' -o -name '*.dmg' \) -print
