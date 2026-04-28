# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A keyboard tool: user types `;en` / `;ja` / `;zh` (ISO-639-1 codes) in any text field, the OS input source switches and the prefix is swallowed. Same trigger across Mac and Windows so the user doesn't have to remember platform-specific IME shortcuts.

- **macOS** runs in **Slipkey.app** — a SwiftPM AppKit/SwiftUI app with status-bar icon, settings UI, and the event hook in the **main process**. Single binary, single Accessibility grant, no daemon child process. The architecture mirrors Mos.
- **Windows** still uses the Rust **imeswitchd** daemon. Compiles for `x86_64-pc-windows-msvc`; runtime tested on a real Windows box is still pending.

## Commands

```bash
# Slipkey (macOS app) — Swift
swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift
swift build -c release --package-path bins/slipkey-app --scratch-path target/slipkey-swift
bash scripts/package-macos.sh                       # full bundle + zip pipeline

# Rust workspace (Windows daemon + shared core)
cargo test --workspace                              # state-machine tests + windows tests
cargo check -p imeswitchd --target x86_64-pc-windows-msvc

# Run one core test by name
cargo test -p imeswitch-core full_trigger_ja
```

When iterating on Slipkey, **always kill the running one first**:

```bash
pkill -x Slipkey
```

## Architecture

### Why a keycode-level hook, not a text watcher

The non-obvious core insight: any approach that reads typed *characters* is dead on arrival. In Chinese/Japanese IMEs, `;en` becomes `；えん` / composition-buffer content before anyone at the application level sees it. The hook MUST run at the HID keycode layer (`CGEventTap(HIDEventTap)` on macOS, `WH_KEYBOARD_LL` on Windows), recognize the sequence by *virtual keycodes*, and consume those keycodes before the IME converts them.

### Layout

- `bins/slipkey-app/` — macOS native app (Swift). All macOS-specific logic lives here, ported from the deleted `crates/imeswitch-macos/` Rust crate.
  - `Sources/SlipkeyApp/Hook/` — `HookKey`, `Keycode`, `StateMachine`, `IMEManager` (Carbon TIS), `Composition` (AX), `EventHook` (CGEventTap)
  - `Sources/SlipkeyApp/Services/` — `HookService`, `AccessibilityService`, `InputSourceService` (uses `IMEManager.listAll`), `LoginItemService`
  - `Sources/SlipkeyApp/App/` — `AppDelegate`, `AppState`, `StatusItemManager`, `WindowManager`
  - `Sources/SlipkeyApp/Views/SettingsView.swift` — SwiftUI settings
  - `Tests/SlipkeyAppTests/` — 27 tests covering `HookKey`, `Keycode`, `StateMachine` (13 ports of the original Rust tests), and `Composition.shouldDefer`
- `crates/imeswitch-core/` — Pure-Rust state machine. Used by the Windows daemon. The macOS Slipkey app has its own (Swift) port of the same algorithm; the two ports are kept in sync semantically.
- `crates/imeswitch-windows/` — Windows daemon implementation. Depends on `imeswitch-core` + `windows-sys`.
- `bins/imeswitchd/` — Windows-only daemon binary. macOS no longer uses this binary at all; on macOS the CLI prints "use the Slipkey app" and exits.

### State machine contract (mirrored on both sides)

A pure function `onKeydown(HookKey) -> StateMachineResponse` (Swift) / `on_keydown(Key) -> Response` (Rust) where the response has three independent fields:

- `suppress: Bool` — whether the platform hook should drop the current event
- `replay: [HookKey]` — previously-suppressed keys the hook must synth-post BEFORE the current event flows through (used on cancellation — e.g. `;ex` must show `;ex`, not just `x`)
- `switchTo: String?` / `switch: Option<Language>` — if set, hook fires the user's switch callback

The machine is stateful across calls (trie current-node + buffer). `isIdle` exposes whether a trigger is in flight — **mid-sequence trumps every other guard** (modifier filter, composition defer) because silently abandoning a partially-grabbed sequence eats user keystrokes without visible output.

### macOS specifics (in Slipkey, not Rust)

- **CGEventTap location: HID, replay at Session.** Posting replay at HID re-enters our own tap and loops; Session is below our tap so events go to IME/app but don't bounce back. See `EventHook.synthPost`.
- **Tap is created with `CGEvent.tapCreate(tap: .cghidEventTap, options: .defaultTap, ...)`.** On modern macOS, `tapCreate` returns non-nil even WITHOUT Accessibility — it just silently never delivers events. The hook installation log saying "installed" therefore does NOT prove events flow; only granted Accessibility does.
- **Disabled-tap re-enable.** Callback handles `.tapDisabledByTimeout` / `.tapDisabledByUserInput` by re-enabling the tap, otherwise macOS would freeze the hook after a long-running callback or after a focus-stealing input event.
- **Keycodes are US-QWERTY (`kVK_ANSI_*`).** Non-QWERTY users (Dvorak etc.) will get wrong triggers. Not addressed.
- **TIS IDs are per-user.** `SlipkeyConfig.defaults` targets `com.apple.keylayout.ABC` / `com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese` / `com.apple.inputmethod.SCIM.Shuangpin`. Users on 全拼 / Rime / 搜狗 / ATOK override via `~/.config/imeswitch/config.toml` or via the Settings UI.
- **Switching uses `TISSelectInputSource` on the input-mode ID directly** (e.g. `SCIM.Shuangpin`, not the parent `SCIM`). The parent IMs show `IsSelectCapable=false` and TIS won't accept them.

### Two guards in the hook (both pre-state-machine)

1. **Modifier mask** (`EventHook.eventKey`): Shift/Ctrl/Option/Cmd held → treat the event as `.other`. Otherwise `Shift+;` (`:`) would start a trigger, making `:en` in code or `:ja` in tests a phantom switch. CapsLock and Fn are intentionally *not* in the mask.

2. **Composition heuristic** (`Composition.swift`): if the active input source is an IME (any TIS type other than `TISTypeKeyboardLayout`), ask Accessibility for `AXMarkedTextRange` on the focused element. Length>0 → `.active`, ==0 → `.inactive`, missing/non-AXValue → fall through to `AXTextInputMarkedTextMarkerRange` (web views). When AX cannot answer, fall back to a 500ms idle-window heuristic (`Composition.idleThreshold`) so we still avoid eating `;` mid-Chinese-pinyin in opaque controls. The Rust code's candidate-window scan was intentionally NOT ported — AX covers the common case and that scan is expensive.

### Windows specifics (in Rust)

Unchanged from the pre-Slipkey design:
- Win64 only (`x86_64-pc-windows-msvc`).
- Hook via `SetWindowsHookExW(WH_KEYBOARD_LL)`; consume by returning 1, pass via `CallNextHookEx`.
- Modifier guard mirrors macOS (Shift/Ctrl/Alt/Win).
- Replay via `SendInput`, marked with a `dwExtraInfo` magic so re-entry skips the hook.
- Composition detection via IMM32 (`ImmGetContext` + `ImmGetCompositionStringW(GCS_COMPSTR)` on focused HWND) — exact, not a time heuristic.
- HKL switching defaults: `00000409` (US), `00000411` (JP), `00000804` (CN Simplified). Override via `%APPDATA%\imeswitch\config.toml`.
- Daemon must run un-elevated (UIPI).
- Some UWP/sandbox apps don't see low-level hook output — document as unsupported.

## Operational gotchas (macOS)

These bit us during the Slipkey port and will bite anyone iterating on the app, so memorize them:

### TCC permissions and ad-hoc signing

macOS TCC binds the Accessibility grant to the binary's CDHash. **Every Slipkey rebuild changes the CDHash and silently invalidates the grant** — the toggle in System Settings stays "on" but the new binary doesn't actually receive events. Symptom: triggers stop working but `hook installed` still appears in the diag log.

The dev workflow when iterating on Swift code:
1. `pkill -x Slipkey`
2. `swift build -c release --package-path bins/slipkey-app …`
3. Replace the binary inside the deployed `.app`; `xattr -cr` and `codesign --force --sign - <app>`
4. `tccutil reset Accessibility dev.zlb.imeswitch`
5. `open <app>`
6. Re-grant Accessibility in System Settings (remove any stale entry, `+` the new bundle path, toggle on)
7. macOS may not actually relaunch on toggle — `pkill -x Slipkey && open <app>` to be sure

For an end-user shipped via the dist .zip, this is a one-time cost. For development, every rebuild costs you a full re-grant.

### iCloud-synced paths break `codesign --verify --strict`

`~/Desktop` and `~/Documents` are inside iCloud Drive's "Desktop & Documents" sync (when enabled, default on most macs). When a `.app` lives there, iCloud's fileprovider continually adds `com.apple.FinderInfo` and `com.apple.fileprovider.fpfs#P` xattrs to the bundle root, and `codesign --verify --deep --strict` then fails ("Disallowed xattr ... found"). Whether TCC actually rejects an iCloud-tagged bundle is unclear — when triggers stopped working during the port, the more reliable explanation turned out to be the rebuild-invalidation issue above (stale CDHash, not the xattrs).

Either way, don't run from `target/release/bundle/macos/Slipkey.app` directly. The package script (`scripts/package-macos.sh`) handles this: it builds in `target/...`, then `ditto`s a clean copy to `/Applications/Slipkey.app`, strips xattrs there, and re-signs. `/Applications` is admin-writable on a default macOS install, so no sudo is needed for an admin user. Run from `/Applications/Slipkey.app` for daily use; `target/...` is just an intermediate build artifact.

### NSLog from a GUI-launched Slipkey is invisible to `log show`

macOS's privacy-by-default policy filters NSLog output from GUI-launched apps in unified logging. To see Slipkey's runtime logs:
- Launch from terminal: `/Applications/Slipkey.app/Contents/MacOS/Slipkey 2>&1 | tee /tmp/slipkey.log`
- Or write to a file from inside the app (the `AppDelegate.diag` helper writes to `/tmp/slipkey-diag.log` on launch — currently in tree as a debug aid; remove when you're sure of the runtime path)

## File pointers

- Trigger logic itself: `bins/slipkey-app/Sources/SlipkeyApp/Hook/StateMachine.swift` (Swift) and `crates/imeswitch-core/src/state_machine.rs` (Rust). Keep them semantically identical.
- macOS hook + IME plumbing: `bins/slipkey-app/Sources/SlipkeyApp/Hook/`
- Windows hook + IME plumbing: `crates/imeswitch-windows/src/`
- The implementation plan that produced the macOS port: `docs/superpowers/plans/2026-04-27-slipkey-native-hook.md`
