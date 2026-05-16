# Development

This document is the handoff path for a fresh machine. If Slipkey breaks later, start here instead of reconstructing the workflow from old chats.

## Repository Layout

```text
bins/slipkey-app/        macOS app (Swift, SwiftPM)
bins/slipkey-windows/    Windows app (Rust, egui)
crates/imeswitch-core/   Pure trigger state machine
crates/imeswitch-windows/Windows hook + IME switching
crates/imeswitch-tsf-protocol/
                          Shared ABI for TSF helper dispatch
crates/slipkey-tsf-helper/
                          Windows helper DLL for TSF compartment writes
xtask/                   Project automation commands
```

## Fresh Windows Machine

Install:

- Git
- Rust stable with `x86_64-pc-windows-msvc`
- Visual Studio Build Tools with the MSVC C++ toolchain

Then:

```powershell
git clone https://github.com/Oguri701/Slipkey.git
cd Slipkey
rustup target add x86_64-pc-windows-msvc
cargo xtask test-windows
cargo xtask build-windows
```

Expected release binary:

```text
target/x86_64-pc-windows-msvc/release/Slipkey.exe
```

`cargo xtask build-windows` performs the required order:

1. Build `slipkey-tsf-helper`.
2. Copy the helper DLL into `bins/slipkey-windows/embed/slipkey_tsf.dll`.
3. Build `Slipkey.exe` with the DLL embedded.

Do not commit the staged DLL in `bins/slipkey-windows/embed/`.

## Fresh macOS Machine

Install:

- Xcode command line tools
- Rust stable

Then:

```bash
git clone https://github.com/Oguri701/Slipkey.git
cd Slipkey
bash scripts/package-macos.sh
```

The script builds, ad-hoc signs, zips, and installs:

```text
/Applications/Slipkey.app
dist/Slipkey-<version>-macos-arm64.zip
```

If Accessibility stops working after rebuilding:

```bash
tccutil reset Accessibility dev.zlb.imeswitch
open /Applications/Slipkey.app
```

Then re-enable Slipkey in System Settings.

## Common Commands

```bash
cargo test --workspace
cargo xtask test-windows
cargo xtask build-windows
bash scripts/package-macos.sh
```

## Runtime Config

Windows:

```text
%APPDATA%\imeswitch\config.toml
%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll
```

macOS:

```text
~/.config/imeswitch/config.toml
```

Use **Detect** in Settings after moving to a new machine. Input source IDs are machine-specific.

## Windows Regression Checklist

Run these before a Windows release:

- Launch from an empty folder containing only `Slipkey.exe`.
- Confirm tray icon appears.
- Open Settings from the tray menu, close it, and open it again.
- Right-click tray menu and confirm it does not open Settings by itself.
- Confirm `%LOCALAPPDATA%\Slipkey\slipkey_tsf.dll` is created.
- Delete the helper DLL, restart Slipkey, and confirm it is recreated.
- Test `;en`, `;ja`, and `;zh` in Notepad.
- Test `;en` and `;ja` in VS Code.
- Test a UWP/system surface such as Windows Settings; helper injection may be refused, but Slipkey should not disturb the app.
- On Japanese and Chinese keyboards, confirm only the configured physical leader key starts a trigger.
- Double-click `Slipkey.exe` twice and confirm only one instance remains active.

Known edge case: on a cold Windows session, the first switch from Chinese IME to Japanese IME may land in Japanese alphanumeric mode and require a second `;ja`. This appears related to first-time TSF profile activation timing.

## macOS Regression Checklist

- Accessibility permission flow opens Settings when permission is missing.
- Status bar menu opens Settings and quits the app.
- Closing Settings does not quit the app.
- `;en`, `;ja`, and `;zh` work in TextEdit.
- Login item toggle persists.
- Support author panel renders correctly.

## Release Checklist

1. Update the workspace version in `Cargo.toml`.
2. Update macOS bundle version behavior if needed.
3. Run the Windows and macOS regression checklists on real machines.
4. Commit all source changes.
5. Build release packages locally on the target platform and test the exact zip before uploading:

macOS:

```bash
VERSION=0.1.8 MAC_ONLY=1 bash scripts/package-macos.sh
```

Windows:

```powershell
cargo xtask build-windows
```

6. Create and push a `v*` tag:

```bash
git tag -a v0.1.8 -m "Slipkey v0.1.8"
git push origin main
git push origin v0.1.8
```

7. Create or edit the GitHub Release manually and upload only the locally tested assets:

```text
Slipkey-<version>-macos-arm64.zip
Slipkey-<version>-windows-x64.zip
```

Do not commit release zips to the repository. GitHub Actions intentionally does not build release packages; macOS and Windows packages must be produced on their respective local machines after testing.

## Bug Reports

Useful information:

- OS version
- Slipkey version
- Keyboard hardware layout
- Active IMEs and source IDs from Settings
- Exact trigger typed
- Foreground app
- Whether it happens only on first run or every time
- `%APPDATA%\imeswitch\config.toml` with personal paths removed
