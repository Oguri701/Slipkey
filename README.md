# Slipkey

Type a short code in any text field to switch the OS input method. Works before Chinese/Japanese IMEs convert keystrokes into composed characters.

```
;en  →  English / ABC
;ja  →  Japanese
;zh  →  Chinese
```

- **macOS** — native SwiftPM app (`Slipkey.app`), single Accessibility grant, status-bar icon, settings UI
- **Windows** — Rust tray app (`Slipkey.exe`), egui settings UI, config via TOML

---

## macOS

### Requirements

- macOS 13+, Apple Silicon (arm64)
- Xcode 15+

### Build & install

```bash
bash scripts/package-macos.sh
```

Runs all tests, builds `Slipkey.app`, installs to `/Applications/Slipkey.app`, ad-hoc signs it.

After install:

1. Open `/Applications/Slipkey.app`
2. Go to **System Settings → Privacy & Security → Accessibility**, add Slipkey and toggle on
3. If triggers stop working after a rebuild, run `tccutil reset Accessibility dev.zlb.imeswitch` and re-grant

### Dev iteration

```bash
pkill -x Slipkey
swift build -c release --package-path bins/slipkey-app --scratch-path target/slipkey-swift
swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift

# Deploy the rebuilt binary
cp target/slipkey-swift/release/Slipkey /Applications/Slipkey.app/Contents/MacOS/Slipkey
xattr -cr /Applications/Slipkey.app && codesign --force --sign - /Applications/Slipkey.app
tccutil reset Accessibility dev.zlb.imeswitch
open /Applications/Slipkey.app
# re-grant Accessibility in System Settings
```

### Config

`~/.config/imeswitch/config.toml` — editable via the **Shortcuts** tab or manually:

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

Use **Detect** in Settings to populate the correct source IDs for your machine.

### Uninstall

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

---

## Windows

### Requirements

- Windows 10/11 x64, Rust toolchain with `x86_64-pc-windows-msvc`

### Build

```bash
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
```

Copy `target/x86_64-pc-windows-msvc/release/Slipkey.exe` to the machine and run it.

### Usage

1. Double-click `Slipkey.exe` - a tray icon appears in the notification area
2. Right-click the tray icon and choose **Open Settings** to configure shortcuts
3. Go to the **General** tab and enable **Launch at login**

### Config

`%APPDATA%\imeswitch\config.toml` - editable via the **Shortcuts** tab or manually. Same schema as macOS; `source` values are Windows HKL IDs (`00000409` = US English, `00000411` = Japanese, `00000804` = Chinese Simplified).

### Uninstall

1. Quit via the tray menu
2. Delete `Slipkey.exe`
3. Delete `%APPDATA%\imeswitch\`
4. Optionally remove `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey` from the registry

---

## Architecture

```
bins/
  slipkey-app/          macOS native app (Swift, SwiftPM)
    Sources/SlipkeyApp/
      Hook/             CGEventTap, state machine, Carbon TIS IME switching
      Services/         Accessibility, login item, input source discovery
      App/              AppDelegate, AppState, WindowManager, StatusItemManager
      Views/            SwiftUI settings UI
      Stores/           Config persistence, L10n, UserDefaults
    Tests/              27 unit tests (state machine, keycode, composition)
  slipkey-windows/      Windows native app (Rust, egui)
    src/
      hook_thread.rs    WH_KEYBOARD_LL + PeekMessageW loop
      startup.rs        Registry launch-at-login
      tray.rs           System tray icon + menu
      ui/               egui settings window (General/Shortcuts/About)

crates/
  imeswitch-core/       Pure-Rust state machine shared by platform apps
  imeswitch-windows/    Windows hook + IME switching (WH_KEYBOARD_LL, HKL)

scripts/
  package-macos.sh      Full macOS build pipeline: test → build → bundle → sign → install
```

### Why keycode-level, not text-watching

Any approach that reads typed *characters* fails with CJK IMEs: `;en` becomes `；えん` in the composition buffer before any app-level code sees it. The hook runs at the HID keycode layer (`CGEventTap(HIDEventTap)` on macOS, `WH_KEYBOARD_LL` on Windows), identifies the trigger by virtual keycode, and consumes those events before the IME converts them.

### State machine

`StateMachine.onKeydown(HookKey) → StateMachineResponse` (Swift) / `on_keydown(Key) → Response` (Rust):

| Field | Purpose |
|---|---|
| `suppress` | Drop the current event |
| `replay` | Synth-post previously-suppressed keys (on cancel, `;ex` shows `;ex` not just `x`) |
| `switchTo` | If set, call the IME-switch callback |

### Two pre-state-machine guards (macOS)

1. **Modifier mask** — Shift/Ctrl/Option/Cmd held → treat as `.other`. Prevents `:en` (Shift+;+en) from triggering.
2. **Composition heuristic** — If the active source is an IME, query `AXMarkedTextRange`. Length > 0 → composition active, suppress the trigger. Falls back to a 500 ms idle window for controls that don't expose AX marked-text.
