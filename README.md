# Slipkey

Slipkey is a typed input-method switcher for multilingual users on macOS and Windows.

Type a short trigger in any text field and Slipkey switches the active input method before the current IME converts those keystrokes:

```text
;en  ->  English / ABC
;ja  ->  Japanese
;zh  ->  Chinese
```

It is built for people who move between English, Chinese, Japanese, macOS, and Windows and want one set of muscle memory everywhere.

## Why Slipkey

System input-source shortcuts are easy to forget, easy to mis-hit, and different across platforms. For multilingual users, a wrong input source breaks the sentence and interrupts the thought.

Slipkey keeps switching inside the writing flow: the trigger is visible, memorable, and the same on every supported platform.

| Approach | Where it breaks | Slipkey difference |
| --- | --- | --- |
| OS input-source shortcuts | Different per OS or machine; conflicts with app shortcuts | Same typed trigger on macOS and Windows |
| Text expansion or app shortcuts | Runs after the IME may have composed or consumed the text | Reads key events before IME conversion |
| Toolbar or menu selection | Slow and interrupts typing | Switches without leaving the text field |

## Download

Get the latest build from [GitHub Releases](https://github.com/Oguri701/Slipkey/releases/latest).

- macOS: `Slipkey-*-macos-arm64.zip`
- Windows: `Slipkey-*-windows-x64.zip`

## Platform Support

- **macOS 13+ Apple Silicon**: Swift app, status-bar menu, SwiftUI settings, Accessibility permission.
- **Windows 10/11 x64**: Rust tray app, egui settings, launch-at-login, single-file distribution.

## macOS

### Install

1. Download `Slipkey-*-macos-arm64.zip` from Releases.
2. Unzip it and move `Slipkey.app` to `/Applications`.
3. Open `Slipkey.app`.
4. Enable Slipkey in **System Settings -> Privacy & Security -> Accessibility**.
5. Open **Shortcuts**, click **Detect**, confirm the input sources, then click **Save**.

### Build

```bash
bash scripts/package-macos.sh
```

The script runs tests, builds `Slipkey.app`, installs it to `/Applications/Slipkey.app`, and ad-hoc signs it.

### Config

```text
~/.config/imeswitch/config.toml
```

Use **Detect** in Settings to populate source IDs for your machine.

### Uninstall

```bash
pkill -x Slipkey
rm -rf /Applications/Slipkey.app ~/.config/imeswitch
```

## Windows

### Install

1. Download `Slipkey-*-windows-x64.zip` from Releases.
2. Unzip it and run the single `Slipkey.exe`.
3. Right-click the tray icon and open **Settings**.
4. Configure shortcuts in **Shortcuts**.
5. Enable **Launch at login** in **General** if desired.

On first run, Slipkey provisions a helper file at:

```text
%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll
```

The helper is embedded in `Slipkey.exe` and is used for TSF-level Microsoft Japanese IME mode switching. Slipkey manages this file automatically.

Windows SmartScreen may show **Windows protected your PC** on first run because Slipkey is not yet code-signed. Click **More info -> Run anyway**.

### Build

```powershell
cargo xtask build-windows
```

Build output:

```text
target/x86_64-pc-windows-msvc/release/Slipkey.exe
```

### Config

```text
%APPDATA%\imeswitch\config.toml
```

Example:

```toml
leader = ";"

[[mappings]]
language = "en"
prefix = "en"

[[mappings]]
language = "ja"
prefix = "ja"
source = "04110411"

[[mappings]]
language = "zh"
prefix = "zh"
source = "08040804"
```

Use **Detect** in Settings to fill the correct source IDs for the current machine.

### Uninstall

1. Quit Slipkey from the tray menu.
2. Delete `Slipkey.exe`.
3. Delete `%APPDATA%\imeswitch\`.
4. Delete `%LOCALAPPDATA%\Slipkey\`.
5. Optionally remove `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey`.

## Architecture

```text
bins/
  slipkey-app/          macOS native app (Swift, SwiftPM)
  slipkey-windows/      Windows native app (Rust, egui)

crates/
  imeswitch-core/         Pure state machine shared by platform apps
  imeswitch-windows/      Windows hook, HKL switching, TSF dispatch
  imeswitch-tsf-protocol/ Shared ABI between Slipkey.exe and helper DLL
  slipkey-tsf-helper/     Short-lived TSF helper DLL injected into the
                          focused GUI thread when Windows needs an
                          authoritative IME compartment write

xtask/                  Cross-platform project commands
scripts/                macOS packaging helper
```

### Keycode-Level Detection

Watching typed characters is too late for CJK IMEs: the IME may compose or consume `;en` before the application sees it.

Slipkey works at the key event layer instead:

- macOS: `CGEventTap(HIDEventTap)`
- Windows: `WH_KEYBOARD_LL`

### Windows Two-Stage Switching

Windows CJK IMEs have two axes: keyboard layout and internal conversion mode. Slipkey handles both:

1. `WM_INPUTLANGCHANGEREQUEST` switches the focused window's HKL.
2. `slipkey_tsf.dll` writes TSF compartments inside the focused GUI thread.

Some protected or UWP surfaces may refuse helper injection. Slipkey logs and continues without disturbing the foreground app.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for fresh-machine setup, build commands, tests, release steps, and debugging notes.

## License

MIT. See [LICENSE](LICENSE).
