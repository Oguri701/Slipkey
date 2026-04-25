# Slipkey

> Source tree, binary, and config directory keep the legacy `imeswitch` name for now.

Type a leader plus a language prefix in any text field to switch the OS input source. The default mappings are:

```text
;en -> English / ABC
;ja -> Japanese
;zh -> Chinese
```

The macOS build works at the physical keycode layer, before Chinese/Japanese IMEs turn the typed keys into composition text. The core trigger engine supports arbitrary two-letter language codes and alphanumeric prefixes.

## Build

```bash
cargo test --workspace
cargo build --release
```

Run the CLI daemon:

```bash
./target/release/imeswitchd
```

List installed macOS input sources:

```bash
./target/release/imeswitchd list
```

Generate a config from installed macOS input sources:

```bash
./target/release/imeswitchd wizard
```

## Config

The macOS config lives at:

```text
~/.config/imeswitch/config.toml
```

Schema v2:

```toml
leader = ";"

[[mappings]]
language = "en"
prefix = "en"
source = "com.apple.keylayout.ABC"

[[mappings]]
language = "ja"
prefix = "ja"
source = "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"

[[mappings]]
language = "zh"
prefix = "zh"
source = "com.apple.inputmethod.SCIM.Shuangpin"
```

Old v1 configs using top-level `en`, `ja`, and `zh` keys are migrated automatically on startup. The old file is backed up as `config.toml.v1.bak`.

## macOS App

The `.app` target is in `bins/imeswitch-app`. It runs without a Dock icon (`LSUIElement=true`), opens settings with `Command Option Comma`, can show or hide a menu bar icon, and exposes a login-item toggle.

Build the app crate:

```bash
cargo check -p imeswitch-app
```

Build a DMG:

```bash
cargo install tauri-cli --version '^2'
./scripts/package-macos.sh
```

The script runs tests, builds the Tauri DMG, then ad-hoc signs the `.app` with `codesign --sign -`.

## Install

1. Open the DMG.
2. Drag `Slipkey.app` to `/Applications`.
3. Launch it once.
4. Grant Accessibility permission in System Settings -> Privacy & Security -> Accessibility.
5. Use `Command Option Comma` to open settings if the app is hidden.

If macOS blocks the unsigned app, right-click `Slipkey.app`, choose Open, then confirm.

## Remove

Quit the app, then remove:

```bash
rm -rf /Applications/Slipkey.app
rm -rf ~/.config/imeswitch
```

If login item was enabled, turn it off in the app before deleting it, or remove the related LaunchAgent from `~/Library/LaunchAgents`.

## Windows

The Windows CLI implementation compiles for win64:

```bash
cargo check -p imeswitchd --target x86_64-pc-windows-msvc
```

It still needs runtime testing on a real Windows machine.
