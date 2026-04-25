# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A keyboard daemon: user types `;en` / `;ja` / `;zh` (ISO-639-1 codes) in any text field, the OS input source switches and the prefix is swallowed. The point is that the same trigger works across Mac and Windows so the user doesn't have to remember platform-specific IME shortcuts. macOS is implemented (M0-M2 + guards); Windows M1 has a first win64 implementation and compiles for `x86_64-pc-windows-msvc`, but still needs runtime testing on a real Windows machine.

## Commands

```bash
# workspace build / test
cargo build --release
cargo test --workspace
cargo check -p imeswitchd --target x86_64-pc-windows-msvc

# run core-only tests (platform-independent state machine)
cargo test -p imeswitch-core

# run one test by name
cargo test -p imeswitch-core full_trigger_ja

# daemon
./target/release/imeswitchd           # run
./target/release/imeswitchd list      # dump all TIS input sources / Windows HKLs
./target/release/imeswitchd init      # write a config.toml template
RUST_LOG=debug ./target/release/imeswitchd   # full per-keydown trace
```

When iterating on the daemon, **always kill the running one first**:

```bash
pkill -x imeswitchd
```

A stale daemon is the most common source of "my fix isn't working" false alarms — macOS permissions are per-binary-hash, but if the old process is still live it keeps handling events and your new build never runs.

## Architecture

### Why a keycode-level hook, not a text watcher

The non-obvious core insight: any approach that reads typed *characters* is dead on arrival. In Chinese/Japanese IMEs, `;en` becomes `；えん` / composition-buffer content before anyone at the application level sees it. The hook MUST run at the HID keycode layer (`CGEventTap(HIDEventTap)` on macOS, `WH_KEYBOARD_LL` on Windows), recognize the sequence by *virtual keycodes*, and consume those keycodes before the IME converts them. Everything in this repo is shaped by that constraint.

### Crate layout

- `crates/imeswitch-core`: platform-agnostic state machine (`StateMachine`, `Key`, `Language`, `Response`). Pure Rust, no I/O, no syscalls. This is what M1 Windows reuses unchanged.
- `crates/imeswitch-macos`: macOS implementation. Depends on `imeswitch-core` + `core-graphics`, `core-foundation`, Carbon (TIS) FFI.
- `crates/imeswitch-windows`: Windows implementation. Depends on `imeswitch-core` + `windows-sys`, using `WH_KEYBOARD_LL`, `SendInput`, IMM32 composition checks, and HKL switching.
- `bins/imeswitchd`: glue binary (`main.rs` only). Subcommand dispatch (`run` / `list` / `init`), config load, wires the platform hook to a platform switcher.

### State machine contract (`imeswitch-core`)

A pure function `on_keydown(Key) -> Response` where `Response` has three independent fields:

- `suppress: bool` — whether the platform hook should drop the current event
- `replay: Vec<Key>` — previously-suppressed keys the hook must synth-post BEFORE letting the current event through (used on sequence cancellation — e.g. `;ex` must show `;ex`, not just `x`)
- `switch: Option<Language>` — if set, hook fires the user's `on_switch` callback

The machine is stateful across calls (tracks leader / AfterE / AfterJ / AfterZ). `is_idle()` exposes whether a trigger is in flight — **mid-sequence trumps every other guard** (modifier filter, composition defer) because silently abandoning a partially-grabbed sequence eats user keystrokes without visible output.

### macOS specifics

- **Suppression only works on `core-graphics >= 0.25`**. The 0.24 `CGEventTap` binding has a silent bug: returning `None` from the callback falls back to the original event pointer, so events you "dropped" still flow through. 0.25 uses `CallbackResult::{Keep, Drop, Replace}`. Do not downgrade this dep.
- **Replay is posted at `CGEventTapLocation::Session`, not HID.** Posting at HID re-enters our own tap and loops. Session is below our tap so the events go to IME/app but don't bounce back.
- **Keycodes are US-QWERTY (`kVK_ANSI_*`).** Non-QWERTY users (Dvorak etc.) will get wrong triggers. Not addressed.
- **TIS IDs are per-user.** `Mapping::default()` targets `com.apple.keylayout.ABC` / `com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese` / `com.apple.inputmethod.SCIM.Shuangpin`. Users on 全拼 / Rime / 搜狗 / ATOK override via `~/.config/imeswitch/config.toml`. `imeswitchd list` is the canonical source of truth for what IDs actually exist on a given machine.
- **Switching uses `TISSelectInputSource` on the input-mode ID directly** (e.g. `SCIM.Shuangpin`, not the parent `SCIM`). The parent IMs show `IsSelectCapable=false` and TIS won't accept them.

### Two guards in the hook (both pre-state-machine)

1. **Modifier mask** (`has_blocking_modifier`): Shift/Ctrl/Option/Cmd held → treat the event as `Key::Other`. Otherwise `Shift+;` (`:`) would start a trigger, making `:en` in code or `:ja` in tests a phantom switch. CapsLock and Fn are intentionally *not* in the mask.

2. **Composition heuristic**: if the current input source is an IME (any `TISType*` except `TISTypeKeyboardLayout`) AND the last keydown was within 500ms AND the state machine is idle, the hook returns `Keep` without feeding the event to the state machine. Goal: don't steal `;` mid-Chinese-pinyin. The 500ms threshold is a pragmatic guess — it's wrong if the user pauses mid-composition for longer, but AX `AXMarkedTextRange` polling on every keydown was judged too costly for M0/M2.

### The `dispatch` module

`crates/imeswitch-macos/src/dispatch.rs` contains a minimal libdispatch FFI (`async_main`). **Currently unused but wired into `lib.rs`.** It exists for an experiment: moving the `TISSelectInputSource` call out of the CGEventTap callback onto the next main-runloop tick so `kTISNotifySelectedKeyboardInputSourceChanged` has time to propagate before the next keystroke. The switching-visually-works-but-typing-stays-Latin bug (see below) may or may not be a timing race; if you take that experiment on, the Arc-refactor needed to share the on_switch closure across the defer is the main work.

## Known open issues

### Intermittent "switch succeeded visually but typing stays Latin"

User reports: after `;zh` or `;ja` the menu-bar IME indicator changes correctly, but subsequent keystrokes in the focused text field produce Latin letters instead of composed Chinese/Japanese. **Sometimes deterministic per-window** (one of ZH or JA is "stuck" for the lifetime of a window, refreshing the window may flip which one), sometimes timing-dependent.

Diagnostic already added: `main.rs` logs `switch {Lang}: {before_id} -> {after_id}` using `TISCopyCurrentKeyboardInputSource` before and after each switch. `RUST_LOG=debug` also prints per-keydown state machine decisions. Next session: have the user reproduce + capture the log, check whether `after_id` matches the requested ID. If yes, the race is downstream of TIS (focused app's NSTextInputContext cache); try the dispatch_async path. If no, TIS isn't actually selecting what we asked for.

## Work plan

### Done

- **M0** — macOS PoC: CGEventTap hook, state machine, TIS switcher, `;en`/`;ja`/`;zh` triggers.
- **M2** — TOML config at `~/.config/imeswitch/config.toml` (XDG_CONFIG_HOME honoured); `list` and `init` subcommands; malformed file → warn + defaults.
- **Quality fixes** — modifier-guard, composition heuristic, full debug logging of keydown decisions.

### Deferred (not doing for now per user)

- **M3 Tray icon** — skipped, macOS already shows the input source in the menu bar.

### Windows specifics

- **Win64 only.** The supported target is `x86_64-pc-windows-msvc`; do not spend engineering time on 32-bit Windows unless a real user appears with that need.
- **Hooking uses `SetWindowsHookExW(WH_KEYBOARD_LL)`.** The hook consumes trigger keys by returning non-zero and passes other keys through with `CallNextHookEx`.
- **Modifier guard mirrors macOS.** Shift/Ctrl/Alt/Win held means the event becomes `Key::Other`, so `Shift+;` (`:`) does not start a trigger.
- **Replay uses `SendInput` with a `dwExtraInfo` magic marker.** Replayed cancellation keys skip the hook when they re-enter.
- **Composition detection uses IMM32.** `ImmGetContext` + `ImmGetCompositionStringW(GCS_COMPSTR)` on the focused HWND is used instead of the macOS time heuristic.
- **Switching uses HKL IDs.** Defaults are `00000409` (US English), `00000411` (Japanese), `00000804` (Chinese Simplified, PRC). Users can override these in `%APPDATA%\imeswitch\config.toml`.
- **Run un-elevated.** An elevated daemon cannot reliably send input to non-elevated apps because of UIPI.

### M1 — Windows parity

Status: first implementation is in place and cross-checks with `cargo check -p imeswitchd --target x86_64-pc-windows-msvc`. It still needs functional testing on Windows for real hook behavior, IME switching, composition detection, and app compatibility.

Scope: reach functional parity with the current macOS build on Windows, sharing `imeswitch-core` unchanged.

New crate `crates/imeswitch-windows/` mirroring `imeswitch-macos/`:

- `keymap.rs` — `VK_OEM_1` (`;`=0xBA), `VK_A`=0x41 … `VK_Z`=0x5A. Same shape as macOS keymap, different constants.
- `hook.rs` — `SetWindowsHookExW(WH_KEYBOARD_LL, …)`. In the callback:
  - Use `KBDLLHOOKSTRUCT::vkCode` for the keycode.
  - Check modifier state via `GetAsyncKeyState(VK_SHIFT|VK_CONTROL|VK_MENU|VK_LWIN|VK_RWIN)` (low-level hook doesn't expose flags in the struct) — mirrors the macOS modifier guard.
  - Consume the event by returning 1 (non-zero) from the hook proc; pass through with `CallNextHookEx`.
  - Replay on cancel: `SendInput(INPUT_KEYBOARD[])`. Mark with `INPUT.ki.dwFlags = KEYEVENTF_SCANCODE` or a custom `dwExtraInfo` magic so the replayed events are skipped when re-entering our own hook.
  - Message pump: `GetMessageW` loop on a dedicated thread (WH_KEYBOARD_LL callbacks run on the thread that installed the hook AND that thread must pump messages).
- `ime.rs` — `LoadKeyboardLayoutW(L"00000409", KLF_ACTIVATE)` etc., or more robust: `PostMessage(HWND_BROADCAST, WM_INPUTLANGCHANGEREQUEST, 0, hkl)`. Language IDs:
  - en: `00000409` (US English)
  - ja: `00000411` (Japanese)
  - zh: `00000804` (Chinese Simplified, PRC)
  - Users on MS Pinyin/Sogou/Rime override via the same `config.toml`.
- `composition.rs` — `ImmGetContext(hwnd) + ImmGetCompositionStringW(hIMC, GCS_COMPSTR, …)`: non-empty buffer on focused window means composition is active. This is cheap and EXACT — use it instead of the time-based heuristic on Windows. Get focused HWND via `GetForegroundWindow` + `GetGUIThreadInfo`.

`bins/imeswitchd/Cargo.toml` already has a `[target.'cfg(target_os = "macos")']` block; add the Windows counterpart. `main.rs` `#[cfg(target_os = "windows")]` branch analogous to the macOS one.

TOML schema stays flat for now. If Mac and Win IDs diverge meaningfully per-user we can add `[macos]` / `[windows]` sections later — YAGNI until someone hits it.

Windows-specific gotchas to watch for:
- Some UWP/sandbox apps don't see low-level hook output; document as unsupported like we do for macOS secure input.
- `WM_INPUTLANGCHANGEREQUEST` is per-thread, so `HWND_BROADCAST` is needed for app-wide switching; retry with `SendMessageTimeoutW` if the first attempt lags.
- No "Accessibility permission" dance on Win but the daemon needs to run **un-elevated** — if it runs elevated it can't send input to non-elevated apps (UIPI).

### Other candidates (ordered)

1. **AX-based composition check for macOS (M4 v2)** — replace the 500ms heuristic with `AXUIElementCopyAttributeValue(focused, kAXSelectedTextRangeAttribute)` + marked-text detection. More correct, no magic threshold. Only worth doing if users find the current heuristic misfiring often.
2. **Settings UI (M5)** — Tauri panel for editing the mapping and reviewing which sources are installed. Non-essential; config file is fine.
3. **Configurable trigger character** — currently hard-coded to `;` via `KC_SEMICOLON`. Requires threading a keycode through `StateMachine::with_leader(keycode)` and the keymap. Only if a user asks.

## File pointers (for orientation, not a map)

- Trigger logic itself is `crates/imeswitch-core/src/state_machine.rs` — start here for anything about trigger behavior.
- Platform glue lives in the `imeswitch-macos` crate; each file has a purpose-stating top comment.
- The running plan document with rationale for design choices is at `/Users/zlb/.claude/plans/mdc-mac-win-jp-zh-cn-en-expressive-waffle.md`.
