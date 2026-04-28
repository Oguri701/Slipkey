# Windows GUI — Design Spec

**Date:** 2026-04-28  
**Status:** Approved

## Goal

Bring the Windows version to full feature parity with the macOS Slipkey app:
system tray icon, settings window (three tabs), launch at login, and correct
composition detection for modern TSF-based IMEs (Microsoft Pinyin, Microsoft
Japanese IME).

## What Changes

| Area | Change |
|---|---|
| `bins/imeswitchd/` | **Deleted** — replaced by `bins/slipkey-windows/` |
| `crates/imeswitch-windows/src/composition.rs` | Add CJK-aware TSF fallback |
| `crates/imeswitch-windows/src/hook.rs` | Wire TSF fallback into `update_possible_composition` |
| `crates/imeswitch-windows/src/ime.rs` | Enrich `SourceInfo` with name + language; add `detect_default_sources()` |
| `bins/slipkey-windows/` | **New binary** — tray app with egui settings window |

---

## Section 1 — TSF Composition Detection Fix

### Problem

Modern Microsoft IMEs (Pinyin, Japanese, Korean) use the Text Services Framework
(TSF). `ImmGetCompositionStringW` always returns 0 for TSF IMEs, so
`is_composing()` never fires during Chinese/Japanese input. Result: `;en` can
trigger mid-composition, eating user keystrokes.

### Fix

**`composition.rs`** — add:

```rust
pub fn is_cjk_ime_active() -> bool
```

Reads the foreground thread's HKL via `GetKeyboardLayout(foreground_tid)`,
extracts the low 16-bit LANGID, and returns `true` for:

| LANGID | Language |
|---|---|
| `0x0411` | Japanese |
| `0x0804` | Chinese Simplified |
| `0x0404` | Chinese Traditional |
| `0x0412` | Korean |

**`hook.rs`** — extend `update_possible_composition`:

```
if did_switch || is_composition_ending_key → possible_composition = false
else if composing || Key::Other           → possible_composition = true
else if is_cjk_ime_active() && Key::AlphaNum → possible_composition = true  ← NEW
```

Composition-ending keys (space, enter, backspace, delete, escape) clear
`possible_composition`, so `;en` becomes available again after the user confirms
or cancels the composition candidate.

This mirrors the macOS 500 ms idle-window fallback: conservative by default,
cleared on explicit commit/cancel.

---

## Section 2 — Input Source Detection

**`ime.rs`** — update `SourceInfo`:

```rust
pub struct SourceInfo {
    pub id: String,       // HKL hex string e.g. "00000411"
    pub name: String,     // e.g. "Japanese" (via GetLocaleInfoW LOCALE_SENGLISHLANGUAGENAME)
    pub language: String, // 2-char ISO code e.g. "ja"
}
```

LANGID → ISO-639-1 mapping for the three default languages (en/ja/zh); all
others fall back to the Windows locale name.

Add:

```rust
pub fn detect_default_sources() -> Vec<SourceInfo>
```

Returns installed layouts that match the default en/ja/zh language codes —
used by the settings window's **Detect** button.

---

## Section 3 — `bins/slipkey-windows/`

### Directory layout

```
bins/slipkey-windows/
  Cargo.toml
  src/
    main.rs          — entry point
    app.rs           — AppState (Arc<Mutex<>> shared between threads)
    hook_thread.rs   — background thread: SetWindowsHookExW + GetMessageW loop
    startup.rs       — registry-based launch at login
    tray.rs          — Shell_NotifyIcon / tray-icon integration
    ui/
      mod.rs         — SettingsWindow (egui App impl), tab routing
      general.rs     — General tab
      shortcuts.rs   — Shortcuts tab
      about.rs       — About tab
  assets/
    icon.ico         — converted from bins/slipkey-app/Resources/icon.icns
```

### Dependencies (`Cargo.toml`)

```toml
[dependencies]
egui        = "0.31"
eframe      = "0.31"
tray-icon   = "0.21"
imeswitch-core    = { path = "../../crates/imeswitch-core" }
imeswitch-windows = { path = "../../crates/imeswitch-windows" }
windows-sys = { version = "0.59", features = [...] }
env_logger  = "0.11"
log         = "0.4"
anyhow      = "1"
serde       = { version = "1", features = ["derive"] }
toml        = "0.8"
```

### Thread model

```
Main thread   egui event loop — SettingsWindow, tray event polling
Hook thread   SetWindowsHookExW + GetMessageW loop
Shared        Arc<Mutex<AppState>>
Signal        mpsc::channel<HookCmd> — "restart hook with new config"
```

The hook thread owns the `EventHook` RAII guard. On receiving `HookCmd::Restart`,
it drops the old hook and reinstalls with new mappings — same pattern as macOS
`HookService.restart(with:)`.

### AppState

```rust
pub struct AppState {
    pub config: SlipkeyConfig,          // mirrors macOS SlipkeyConfig
    pub detected_sources: Vec<SourceInfo>,
    pub status_message: String,
    pub hook_active: bool,
    pub launch_at_login: bool,
}
```

`SlipkeyConfig` already exists in `imeswitch-windows` as `Mapping`/`Config`;
rename/wrap to align with macOS naming.

### Window behaviour

- **On launch:** window hidden; tray icon visible.
- **Tray left-click / "Open Settings":** `frame.set_visible(true)`.
- **Window close button:** `frame.set_visible(false)` — does not quit.
- **"Quit Slipkey":** `std::process::exit(0)`.
- **Tab switching:** egui `ui.selectable_value` tabs at top, content area below.
  Window height auto-sizes to content (egui handles this naturally with
  `auto_sized()` window mode).

### Tray menu

```
Open Settings
──────────────
Quit Slipkey
```

### Settings — General tab

| Control | Behaviour |
|---|---|
| Launch at login toggle | Calls `startup::set_enabled(bool)` |
| Hook status badge | Green "Active" / Red "Inactive" based on `app_state.hook_active` |

No "show menu bar icon" toggle (tray IS the UI on Windows; always visible).

### Settings — Shortcuts tab

Mirrors macOS ShortcutSettingsView:

| Control | Behaviour |
|---|---|
| Leader key text field (1 char) | Updates `config.leader` |
| Mappings table | Language name \| Prefix text field \| Source `ComboBox` |
| Detect button | Calls `detect_default_sources()`, updates dropdown options |
| Reset to defaults | Resets config to `Mapping::default()` |
| Save (Ctrl+S) | Saves TOML, sends `HookCmd::Restart` |

Source dropdown shows `"{name} ({id})"` — e.g. `"Japanese (00000411)"`.

### Settings — About tab

```
[icon 64×64]  Slipkey
              Switch input methods by typing.
              v0.1.0  ·  © 2026 zlb
──────────────────────────────────────────
[View on GitHub]
```

---

## Section 4 — Launch at Login (`startup.rs`)

```rust
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "Slipkey";

pub fn is_enabled() -> bool
pub fn set_enabled(enabled: bool) -> anyhow::Result<()>
```

`set_enabled(true)` writes `Slipkey = "<exe_path>"` to `HKCU\...\Run`.  
`set_enabled(false)` deletes the value.  
`exe_path` = `std::env::current_exe()`.

Uses `windows-sys` `RegOpenKeyExW` / `RegSetValueExW` / `RegDeleteValueW`.

---

## Section 5 — Config file

Unchanged: `%APPDATA%\imeswitch\config.toml`. Same TOML schema as macOS
`~/.config/imeswitch/config.toml`. Existing v1→v2 migration in `config.rs`
is preserved.

---

## Section 6 — Remove `bins/imeswitchd/`

Delete `bins/imeswitchd/` entirely. Remove from root `Cargo.toml` workspace
`members` list.

---

## Out of Scope

- Native Win32 / WinUI3 dialogs — settings window uses egui renderer
- Keyboard shortcut to open settings window (macOS has Cmd+, convention; Windows has no equivalent standard)
- Code signing / MSIX packaging (out of scope for now)
- UWP / sandboxed app support (documented limitation, unchanged)
