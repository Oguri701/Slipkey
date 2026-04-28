# Windows GUI (Slipkey.exe) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `Slipkey.exe` — a Windows tray app with egui settings window providing full feature parity with the macOS `Slipkey.app`.

**Architecture:** Enhance `imeswitch-windows` crate with TSF-aware composition detection, enriched `SourceInfo`, config persistence, and an updatable hook API. A new `slipkey-windows` binary hosts an egui settings window on the main thread plus a background hook thread, sharing state via `Arc<Mutex<AppState>>` and a `mpsc` channel for hook restart commands.

**Tech Stack:** Rust, egui 0.31, eframe 0.31, tray-icon 0.21, image 0.25, open 5, windows-sys 0.61

---

## File Map

### Modified
| File | Change |
|---|---|
| `crates/imeswitch-windows/Cargo.toml` | Add `Win32_Globalization` to windows-sys features |
| `crates/imeswitch-windows/src/composition.rs` | Add `is_cjk_ime_active()` |
| `crates/imeswitch-windows/src/hook.rs` | Refactor `HookState` (Arc switcher, deadlock-safe), add `update_config`, wire TSF fix |
| `crates/imeswitch-windows/src/ime.rs` | Add `name`/`language` to `SourceInfo`, update `list_all_sources`, add `detect_default_sources` |
| `crates/imeswitch-windows/src/config.rs` | Add `save()` and `save_to()` |
| `Cargo.toml` (root) | Replace `bins/imeswitchd` with `bins/slipkey-windows` |

### Deleted
- `bins/imeswitchd/` (entire directory)

### Created
| File | Purpose |
|---|---|
| `bins/slipkey-windows/Cargo.toml` | New binary manifest |
| `bins/slipkey-windows/assets/icon.png` | Copied from `bins/slipkey-app/Resources/icon.png` |
| `bins/slipkey-windows/src/main.rs` | Entry point: state init, hook thread, tray, egui loop |
| `bins/slipkey-windows/src/app.rs` | `AppState` — shared mutable state |
| `bins/slipkey-windows/src/hook_thread.rs` | Background thread: `PeekMessageW` loop + hook restart |
| `bins/slipkey-windows/src/startup.rs` | Registry-based launch at login |
| `bins/slipkey-windows/src/tray.rs` | System tray icon + menu |
| `bins/slipkey-windows/src/ui/mod.rs` | `SettingsWindow` (egui `App` impl), tab routing |
| `bins/slipkey-windows/src/ui/general.rs` | General tab |
| `bins/slipkey-windows/src/ui/shortcuts.rs` | Shortcuts tab |
| `bins/slipkey-windows/src/ui/about.rs` | About tab |

---

## Task 1 — TSF composition detection (`composition.rs`)

**Files:**
- Modify: `crates/imeswitch-windows/src/composition.rs`

Modern Microsoft IMEs (Pinyin, Japanese) use TSF; `ImmGetCompositionStringW` always returns 0. Add `is_cjk_ime_active()` that reads the foreground thread's keyboard layout and returns `true` for CJK LANGID codes. This is called from `hook.rs` (Task 4) to set `possible_composition` on alpha key presses.

- [ ] **Append to `composition.rs`**

```rust
/// Returns true if the foreground thread has a CJK keyboard layout active.
/// Used as a fallback when IMM32 cannot detect TSF-based composition (Microsoft
/// Pinyin, Microsoft Japanese IME, etc.).
pub fn is_cjk_ime_active() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    unsafe {
        let fg = GetForegroundWindow();
        let tid = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, std::ptr::null_mut())
        };
        let hkl = GetKeyboardLayout(tid);
        let langid = (hkl as usize) & 0xFFFF;
        // 0x0411 Japanese, 0x0804 Chinese Simplified,
        // 0x0404 Chinese Traditional, 0x0412 Korean
        matches!(langid, 0x0411 | 0x0804 | 0x0404 | 0x0412)
    }
}
```

- [ ] **Verify cross-compile**

```bash
cargo check -p imeswitch-windows --target x86_64-pc-windows-msvc
```

Expected: `Finished` with no errors.

- [ ] **Commit**

```bash
git add crates/imeswitch-windows/src/composition.rs
git commit -m "feat(windows): add is_cjk_ime_active for TSF composition detection"
```

---

## Task 2 — Enrich `SourceInfo` + `detect_default_sources` (`ime.rs`)

**Files:**
- Modify: `crates/imeswitch-windows/src/ime.rs`
- Modify: `crates/imeswitch-windows/Cargo.toml`

Add `name` and `language` fields to `SourceInfo` so the settings UI can show "Japanese (00000411)". Add `detect_default_sources()` for the Detect button.

- [ ] **Add `Win32_Globalization` feature to `crates/imeswitch-windows/Cargo.toml`**

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_Globalization",
    "Win32_System_LibraryLoader",
    "Win32_UI_Input_Ime",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Write a compile-time struct test first** (add to bottom of `ime.rs`)

```rust
#[test]
fn source_info_has_name_and_language() {
    let s = SourceInfo {
        id: "00000411".to_string(),
        name: "Japanese".to_string(),
        language: "ja".to_string(),
    };
    assert_eq!(s.language, "ja");
    assert_eq!(s.name, "Japanese");
}
```

- [ ] **Run test — expect compile failure (struct missing fields)**

```bash
cargo test -p imeswitch-windows source_info_has_name_and_language 2>&1 | head -20
```

Expected: `error[E0063]: missing fields`

- [ ] **Update `SourceInfo` struct definition** (replace existing)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
}
```

- [ ] **Add helper functions** (before `list_all_sources`)

```rust
fn langid_to_iso(langid: u32) -> String {
    match langid & 0xFFFF {
        0x0409 | 0x0809 | 0x0C09 | 0x1009 | 0x1409 | 0x1809 => "en".to_string(),
        0x0411 => "ja".to_string(),
        0x0412 => "ko".to_string(),
        0x0804 | 0x0404 | 0x0C04 | 0x1404 => "zh".to_string(),
        other => {
            match other & 0x3FF {
                0x09 => "en".to_string(),
                0x11 => "ja".to_string(),
                0x12 => "ko".to_string(),
                0x04 => "zh".to_string(),
                _ => format!("{:04X}", langid),
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn locale_language_name(langid: u32) -> String {
    use windows_sys::Win32::Globalization::GetLocaleInfoW;
    const LOCALE_SENGLISHLANGUAGENAME: u32 = 0x0001_0001;
    let mut buf = vec![0u16; 128];
    let len = unsafe {
        GetLocaleInfoW(langid, LOCALE_SENGLISHLANGUAGENAME, buf.as_mut_ptr(), buf.len() as i32)
    };
    if len > 1 {
        String::from_utf16_lossy(&buf[..len as usize - 1])
    } else {
        format!("{:04X}", langid)
    }
}

#[cfg(not(target_os = "windows"))]
fn locale_language_name(langid: u32) -> String {
    format!("{:04X}", langid)
}
```

- [ ] **Replace `list_all_sources` Windows impl** (replace the existing `#[cfg(target_os = "windows")]` version)

```rust
#[cfg(target_os = "windows")]
pub fn list_all_sources() -> Vec<SourceInfo> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    if count <= 0 {
        return Vec::new();
    }
    let mut layouts = vec![std::ptr::null_mut(); count as usize];
    let actual = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
    layouts
        .into_iter()
        .take(actual.max(0) as usize)
        .map(|hkl| {
            let id = format_hkl(hkl);
            let langid = (hkl as usize & 0xFFFF) as u32;
            SourceInfo {
                id,
                name: locale_language_name(langid),
                language: langid_to_iso(langid),
            }
        })
        .collect()
}
```

- [ ] **Add `detect_default_sources`** (after `list_all_sources`)

```rust
/// Returns installed keyboard layouts whose language matches en, ja, or zh.
/// Used by the Detect button in the settings window.
pub fn detect_default_sources() -> Vec<SourceInfo> {
    list_all_sources()
        .into_iter()
        .filter(|s| matches!(s.language.as_str(), "en" | "ja" | "zh"))
        .collect()
}
```

- [ ] **Run test — expect pass**

```bash
cargo test -p imeswitch-windows source_info_has_name_and_language
```

Expected: `test source_info_has_name_and_language ... ok`

- [ ] **Verify cross-compile**

```bash
cargo check -p imeswitch-windows --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add crates/imeswitch-windows/Cargo.toml crates/imeswitch-windows/src/ime.rs
git commit -m "feat(windows): enrich SourceInfo with name/language, add detect_default_sources"
```

---

## Task 3 — Config persistence (`config.rs`)

**Files:**
- Modify: `crates/imeswitch-windows/src/config.rs`

The settings window needs to save config after editing. Add `save()` and `save_to()`.

- [ ] **Write failing test** (append to `#[cfg(test)]` block in `config.rs`)

```rust
#[test]
fn save_to_and_reload_round_trips() {
    use super::*;
    let tmp = std::env::temp_dir().join("imeswitch-test-save-round-trip.toml");
    let mapping = Mapping::default();
    let config = Config::from_mapping(&mapping);
    save_to(&config, &tmp).expect("save_to failed");
    match load_from(&tmp) {
        LoadOutcome::Loaded { config: loaded, .. } => {
            assert_eq!(loaded.into_mapping(), mapping);
        }
        other => panic!("expected Loaded, got {:?}", other),
    }
    std::fs::remove_file(&tmp).ok();
}
```

- [ ] **Run test — expect compile failure** (`save_to` not defined yet)

```bash
cargo test -p imeswitch-windows save_to_and_reload 2>&1 | head -10
```

- [ ] **Add `save` and `save_to` to `config.rs`** (add after `load_or_default`)

```rust
/// Saves `config` to the default path (`%APPDATA%\imeswitch\config.toml`).
pub fn save(config: &Config) -> anyhow::Result<()> {
    save_to(config, &default_path())
}

/// Saves `config` to an explicit path (used in tests and the settings window).
pub fn save_to(config: &Config, path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create config dir")?;
    }
    let toml = toml::to_string_pretty(config).context("serialize config")?;
    std::fs::write(path, toml).context("write config")?;
    Ok(())
}
```

- [ ] **Run test — expect pass**

```bash
cargo test -p imeswitch-windows save_to_and_reload
```

Expected: `test ... ok`

- [ ] **Commit**

```bash
git add crates/imeswitch-windows/src/config.rs
git commit -m "feat(windows): add config::save and save_to"
```

---

## Task 4 — Refactor `hook.rs` (Arc switcher + `update_config` + TSF fix)

**Files:**
- Modify: `crates/imeswitch-windows/src/hook.rs`

Three changes in one file:
1. Replace `on_switch: Box<dyn FnMut>` with `switcher: Arc<Mutex<ImeSwitcher>>` to allow live config updates without reinstalling the Win32 hook.
2. Fix deadlock: clone the `Arc` while holding the `HOOK_STATE` lock, release the lock, *then* call `switch_to`.
3. Wire in `is_cjk_ime_active` for TSF-IME composition detection.

- [ ] **Replace entire `hook.rs`** with the following

```rust
//! Windows low-level keyboard hook that feeds keydowns into `imeswitch-core`.

use std::sync::{Arc, Mutex, OnceLock};

use imeswitch_core::{Key, Language, StateMachine};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::composition::{is_composing, is_cjk_ime_active};
use crate::ime::{ImeSwitcher, Mapping};
use crate::keymap::{key_to_vk_with_leader, leader_vk_for, vk_to_key_with_leader, VK_SEMICOLON};

const REPLAY_MAGIC: usize = 0x696d_6573_7769_6e36;

static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

#[derive(Debug)]
pub enum HookError {
    AlreadyInstalled,
    StateUnavailable,
    InstallFailed,
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            HookError::AlreadyInstalled => "keyboard hook already installed",
            HookError::StateUnavailable => "keyboard hook state unavailable",
            HookError::InstallFailed => "SetWindowsHookExW(WH_KEYBOARD_LL) failed",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for HookError {}

struct HookState {
    sm: StateMachine,
    /// Shared with the hook thread so config can be swapped without
    /// uninstalling the Win32 hook. Lock order: always HOOK_STATE first,
    /// then switcher — never hold both simultaneously (see handle_keydown).
    switcher: Arc<Mutex<ImeSwitcher>>,
    possible_composition: bool,
    leader_vk: u32,
}

pub struct EventHook {
    hook: HHOOK,
}

impl EventHook {
    /// Install the hook. `switcher` is shared with the caller so
    /// `update_config` can swap the mapping without reinstalling.
    pub fn install_with_mappings<I>(
        leader_vk: u32,
        mappings: I,
        switcher: Arc<Mutex<ImeSwitcher>>,
    ) -> Result<Self, HookError>
    where
        I: IntoIterator<Item = (Language, String)>,
    {
        HOOK_STATE
            .set(Mutex::new(HookState {
                sm: StateMachine::from_mappings(mappings),
                switcher,
                possible_composition: false,
                leader_vk,
            }))
            .map_err(|_| HookError::AlreadyInstalled)?;

        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                std::ptr::null_mut(),
                0,
            )
        };
        if hook.is_null() {
            return Err(HookError::InstallFailed);
        }
        Ok(Self { hook })
    }

    /// Update the state machine and leader key without reinstalling the hook.
    /// Call this after the shared `Arc<Mutex<ImeSwitcher>>` has already been
    /// updated with the new mapping.
    pub fn update_config(&self, mapping: &Mapping) {
        if let Some(state) = HOOK_STATE.get() {
            let mut guard = state.lock().unwrap();
            guard.sm = StateMachine::from_mappings(mapping.trigger_mappings());
            guard.leader_vk = leader_vk_for(mapping.leader()).unwrap_or(VK_SEMICOLON);
        }
    }
}

impl Drop for EventHook {
    fn drop(&mut self) {
        if !self.hook.is_null() {
            unsafe { UnhookWindowsHookEx(self.hook) };
        }
    }
}

pub fn run_message_loop() {
    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if code != HC_ACTION as i32
        || (w_param != WM_KEYDOWN as WPARAM && w_param != WM_SYSKEYDOWN as WPARAM)
    {
        return CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param);
    }

    let kb = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };
    if kb.dwExtraInfo == REPLAY_MAGIC {
        return CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param);
    }

    match handle_keydown(kb.vkCode) {
        Ok(true) => 1,
        Ok(false) => CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param),
        Err(err) => {
            log::error!("keyboard hook error: {}", err);
            CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param)
        }
    }
}

fn handle_keydown(vk: u32) -> Result<bool, HookError> {
    let state = HOOK_STATE.get().ok_or(HookError::StateUnavailable)?;
    let composing = is_composing();
    let has_mod = has_blocking_modifier();

    // Clone the Arc while holding the HOOK_STATE lock, then release the lock
    // before calling switch_to. This prevents deadlock when update_config
    // locks switcher first and then HOOK_STATE.
    let (response, leader_vk, switcher_arc) = {
        let mut guard = state.lock().unwrap();
        let leader_vk = guard.leader_vk;
        let should_defer = guard.sm.is_idle()
            && (composing
                || (guard.possible_composition && !is_composition_ending_key(vk, leader_vk)));

        log::debug!(
            "kd vk={:#04x} composing={} possible={} mod={} defer={}",
            vk, composing, guard.possible_composition, has_mod, should_defer,
        );

        if should_defer {
            update_possible_composition(&mut guard, true, vk, false);
            return Ok(false);
        }

        let key = if has_mod {
            Key::Other
        } else {
            vk_to_key_with_leader(vk, leader_vk)
        };
        let response = guard.sm.on_keydown(key);
        update_possible_composition(&mut guard, composing, vk, response.switch.is_some());

        log::debug!(
            "  -> key={:?} suppress={} replay={:?} switch={:?}",
            key, response.suppress, response.replay, response.switch,
        );

        let switcher_arc = guard.switcher.clone(); // clone Arc, cheap
        (response, leader_vk, switcher_arc)
        // HOOK_STATE lock released here — switcher_arc is now held independently
    };

    if let Some(ref lang) = response.switch {
        // No HOOK_STATE lock held here, so no deadlock with update_config.
        if let Err(e) = switcher_arc.lock().unwrap().switch_to(lang) {
            log::error!("switch failed: {}", e);
        }
    }

    for key in &response.replay {
        if let Some(vk) = key_to_vk_with_leader(*key, leader_vk) {
            send_key(vk);
        }
    }

    Ok(response.suppress)
}

fn has_blocking_modifier() -> bool {
    [
        VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
        VK_MENU, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN,
    ]
    .iter()
    .any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } < 0)
}

fn update_possible_composition(state: &mut HookState, composing: bool, vk: u32, did_switch: bool) {
    if did_switch || is_composition_ending_key(vk, state.leader_vk) {
        state.possible_composition = false;
    } else if composing || vk_to_key_with_leader(vk, state.leader_vk) == Key::Other {
        state.possible_composition = true;
    } else if is_cjk_ime_active() {
        // TSF-based IMEs (Microsoft Pinyin/Japanese) don't report via IMM32.
        // Treat alphanum presses while a CJK layout is active as potential
        // composition input, mirroring the macOS 500 ms idle fallback.
        if matches!(vk_to_key_with_leader(vk, state.leader_vk), Key::AlphaNum(_)) {
            state.possible_composition = true;
        }
    }
}

fn is_composition_ending_key(vk: u32, _leader_vk: u32) -> bool {
    matches!(
        vk,
        x if x == VK_SPACE as u32
            || x == VK_BACK as u32
            || x == VK_DELETE as u32
            || x == VK_ESCAPE as u32
            || x == 0x0D // Enter
    )
}

fn send_key(vk: u32) {
    let mut inputs = [keyboard_input(vk, 0), keyboard_input(vk, KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

fn keyboard_input(vk: u32, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk as u16,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: REPLAY_MAGIC,
            },
        },
    }
}
```

- [ ] **Verify cross-compile**

```bash
cargo check -p imeswitch-windows --target x86_64-pc-windows-msvc
```

Expected: `Finished` with no errors.

- [ ] **Run existing Rust tests** (state machine + config, non-Windows)

```bash
cargo test --workspace
```

Expected: all tests pass (Swift tests won't run — that's expected).

- [ ] **Commit**

```bash
git add crates/imeswitch-windows/src/hook.rs
git commit -m "feat(windows): refactor hook — Arc switcher, update_config, TSF composition fix"
```

---

## Task 5 — Workspace swap: delete `imeswitchd`, scaffold `slipkey-windows`

**Files:**
- Delete: `bins/imeswitchd/`
- Modify: `Cargo.toml` (root)
- Create: `bins/slipkey-windows/Cargo.toml`
- Create: `bins/slipkey-windows/assets/icon.png`

- [ ] **Delete `imeswitchd`**

```bash
rm -rf bins/imeswitchd
```

- [ ] **Update root `Cargo.toml`** — replace `"bins/imeswitchd"` with `"bins/slipkey-windows"`

```toml
[workspace]
resolver = "2"
members = [
    "crates/imeswitch-core",
    "crates/imeswitch-windows",
    "bins/slipkey-windows",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[workspace.dependencies]
imeswitch-core    = { path = "crates/imeswitch-core" }
imeswitch-windows = { path = "crates/imeswitch-windows" }
anyhow      = "1"
thiserror   = "2"
log         = "0.4"
env_logger  = "0.11"

[profile.release]
lto = "thin"
strip = true
```

- [ ] **Create `bins/slipkey-windows/Cargo.toml`**

```toml
[package]
name = "slipkey-windows"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[[bin]]
name = "Slipkey"
path = "src/main.rs"

[dependencies]
imeswitch-core.workspace    = true
imeswitch-windows.workspace = true
anyhow.workspace            = true
log.workspace               = true
env_logger.workspace        = true
egui   = "0.31"
eframe = { version = "0.31", default-features = false, features = ["default_fonts", "glow"] }
tray-icon = "0.21"
image  = { version = "0.25", default-features = false, features = ["png"] }
open   = "5"

[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_System_Registry",
    "Win32_System_Threading",
    "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Copy icon asset**

```bash
mkdir -p bins/slipkey-windows/assets
cp bins/slipkey-app/Resources/icon.png bins/slipkey-windows/assets/icon.png
```

- [ ] **Create empty source stubs** so the workspace compiles

```bash
mkdir -p bins/slipkey-windows/src/ui
```

Create `bins/slipkey-windows/src/main.rs`:
```rust
fn main() {}
```

Create `bins/slipkey-windows/src/ui/mod.rs`:
```rust
```

- [ ] **Verify workspace compiles**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

Expected: `Finished` with no errors.

- [ ] **Commit**

```bash
git add -A
git commit -m "chore: replace imeswitchd with slipkey-windows binary scaffold"
```

---

## Task 6 — `app.rs` (shared state)

**Files:**
- Create: `bins/slipkey-windows/src/app.rs`

- [ ] **Create `bins/slipkey-windows/src/app.rs`**

```rust
use std::sync::{Arc, Mutex};
use imeswitch_windows::config::{load_or_default, Config};
use imeswitch_windows::ime::{detect_default_sources, SourceInfo};

pub struct AppState {
    pub config: Config,
    pub detected_sources: Vec<SourceInfo>,
    pub status_message: String,
    pub hook_active: bool,
    pub launch_at_login: bool,
}

impl AppState {
    pub fn load() -> Self {
        let (mapping, _outcome) = load_or_default();
        AppState {
            config: Config::from_mapping(&mapping),
            detected_sources: detect_default_sources(),
            status_message: String::new(),
            hook_active: false,
            launch_at_login: crate::startup::is_enabled(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
```

- [ ] **Reference from `main.rs`** (add to existing stub)

```rust
mod app;
mod startup;
fn main() {}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/app.rs bins/slipkey-windows/src/main.rs
git commit -m "feat(win-app): add AppState"
```

---

## Task 7 — `startup.rs` (launch at login)

**Files:**
- Create: `bins/slipkey-windows/src/startup.rs`

Writes `Slipkey = <exe path>` to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

- [ ] **Create `bins/slipkey-windows/src/startup.rs`**

```rust
use anyhow::{Context as _, Result};

const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_VALUE: &str = "Slipkey";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
        };
        let mut hkey = std::ptr::null_mut();
        let ret = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_SUBKEY).as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        };
        if ret != 0 {
            return false;
        }
        let mut data_type = 0u32;
        let mut data_size = 0u32;
        let ret = unsafe {
            RegQueryValueExW(
                hkey,
                wide(APP_VALUE).as_ptr(),
                std::ptr::null_mut(),
                &mut data_type,
                std::ptr::null_mut(),
                &mut data_size,
            )
        };
        unsafe { RegCloseKey(hkey) };
        ret == 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
            KEY_SET_VALUE, REG_SZ,
        };
        let mut hkey = std::ptr::null_mut();
        let ret = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_SUBKEY).as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            )
        };
        if ret != 0 {
            anyhow::bail!("RegOpenKeyExW failed: {ret}");
        }
        let value = wide(APP_VALUE);
        let result = if enabled {
            let exe = std::env::current_exe().context("current_exe")?;
            let path = wide(&exe.to_string_lossy());
            unsafe {
                RegSetValueExW(
                    hkey,
                    value.as_ptr(),
                    0,
                    REG_SZ,
                    path.as_ptr() as *const u8,
                    (path.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(hkey, value.as_ptr()) }
        };
        unsafe { RegCloseKey(hkey) };
        // 2 = ERROR_FILE_NOT_FOUND: value already absent, not an error when disabling
        if result != 0 && !(result == 2 && !enabled) {
            anyhow::bail!("registry op failed: {result}");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
        Ok(())
    }
}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/startup.rs
git commit -m "feat(win-app): registry-based launch at login"
```

---

## Task 8 — `hook_thread.rs` (background hook thread)

**Files:**
- Create: `bins/slipkey-windows/src/hook_thread.rs`

Runs `WH_KEYBOARD_LL` + `PeekMessageW` loop on a background thread. Restarts the hook config when `HookCmd::Restart` arrives via channel, without reinstalling the Win32 hook.

- [ ] **Create `bins/slipkey-windows/src/hook_thread.rs`**

```rust
use std::sync::{Arc, Mutex, mpsc};
use imeswitch_windows::config::Config;
use imeswitch_windows::hook::EventHook;
use imeswitch_windows::ime::ImeSwitcher;
use imeswitch_windows::keymap::{leader_vk_for, VK_SEMICOLON};
use crate::app::SharedState;

pub enum HookCmd {
    Restart(Config),
}

pub fn spawn(state: SharedState, rx: mpsc::Receiver<HookCmd>) {
    std::thread::Builder::new()
        .name("slipkey-hook".into())
        .spawn(move || run(state, rx))
        .expect("hook thread spawn failed");
}

fn install(state: &SharedState) -> (Option<EventHook>, Arc<Mutex<ImeSwitcher>>) {
    let config = state.lock().unwrap().config.clone();
    let mapping = config.into_mapping();
    let leader_vk = leader_vk_for(mapping.leader()).unwrap_or(VK_SEMICOLON);
    let switcher = Arc::new(Mutex::new(ImeSwitcher::with_mapping(mapping.clone())));
    let hook = EventHook::install_with_mappings(
        leader_vk,
        mapping.trigger_mappings(),
        switcher.clone(),
    )
    .map_err(|e| log::error!("hook install failed: {e}"))
    .ok();
    (hook, switcher)
}

#[cfg(target_os = "windows")]
fn run(state: SharedState, rx: mpsc::Receiver<HookCmd>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };

    let (maybe_hook, switcher) = install(&state);
    state.lock().unwrap().hook_active = maybe_hook.is_some();

    let hook = match maybe_hook {
        Some(h) => h,
        None => {
            log::error!("hook not installed; hook thread exiting");
            return;
        }
    };

    loop {
        // Pump Windows messages — required for WH_KEYBOARD_LL delivery.
        unsafe {
            let mut msg = std::mem::zeroed::<MSG>();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Check for config restart command.
        match rx.try_recv() {
            Ok(HookCmd::Restart(config)) => {
                let mapping = config.into_mapping();
                // Update switcher first (no HOOK_STATE lock held).
                *switcher.lock().unwrap() = ImeSwitcher::with_mapping(mapping.clone());
                // Then update state machine inside hook (acquires HOOK_STATE lock).
                hook.update_config(&mapping);
                state.lock().unwrap().hook_active = true;
                log::info!("hook config updated: {}", mapping.describe());
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => return,
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(not(target_os = "windows"))]
fn run(state: SharedState, _rx: mpsc::Receiver<HookCmd>) {
    log::warn!("hook thread: non-Windows target, hook disabled");
    state.lock().unwrap().hook_active = false;
}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/hook_thread.rs
git commit -m "feat(win-app): hook background thread with PeekMessage loop and live config restart"
```

---

## Task 9 — `tray.rs` (system tray icon)

**Files:**
- Create: `bins/slipkey-windows/src/tray.rs`

- [ ] **Create `bins/slipkey-windows/src/tray.rs`**

```rust
use tray_icon::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

pub struct Tray {
    #[allow(dead_code)] // must stay alive; dropped = icon disappears
    inner: TrayIcon,
    pub open_id: MenuId,
    pub quit_id: MenuId,
}

impl Tray {
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        let icon = Icon::from_rgba(rgba, width, height).expect("tray icon RGBA");

        let open_item = MenuItem::new("Open Settings", true, None);
        let quit_item = MenuItem::new("Quit Slipkey", true, None);
        let open_id = open_item.id().clone();
        let quit_id = quit_item.id().clone();

        let menu = Menu::new();
        menu.append(&open_item).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();
        menu.append(&quit_item).unwrap();

        let inner = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("Slipkey — Switch input methods by typing")
            .build()
            .expect("tray icon build");

        Self { inner, open_id, quit_id }
    }
}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/tray.rs
git commit -m "feat(win-app): system tray icon with Open Settings / Quit menu"
```

---

## Task 10 — `ui/about.rs`

**Files:**
- Create: `bins/slipkey-windows/src/ui/about.rs`

- [ ] **Create `bins/slipkey-windows/src/ui/about.rs`**

```rust
use eframe::egui;

pub fn show(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if let Some(tex) = icon {
            ui.image((tex.id(), egui::vec2(64.0, 64.0)));
            ui.add_space(12.0);
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Slipkey").size(34.0).strong());
            ui.add_space(2.0);
            ui.label("Switch input methods by typing.");
            ui.add_space(4.0);
            let version = env!("CARGO_PKG_VERSION");
            ui.label(
                egui::RichText::new(format!("v{version}  ·  © 2026 zlb"))
                    .small()
                    .weak(),
            );
        });
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    if ui.button("View on GitHub").clicked() {
        let _ = open::that("https://github.com/Oguri701/imeswitch");
    }
}
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/ui/about.rs
git commit -m "feat(win-app): About tab"
```

---

## Task 11 — `ui/general.rs`

**Files:**
- Create: `bins/slipkey-windows/src/ui/general.rs`

- [ ] **Create `bins/slipkey-windows/src/ui/general.rs`**

```rust
use eframe::egui;
use crate::app::AppState;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);

    // Launch at login
    let mut launch = state.launch_at_login;
    if ui.checkbox(&mut launch, "Launch at login").changed() {
        match crate::startup::set_enabled(launch) {
            Ok(()) => state.launch_at_login = launch,
            Err(e) => state.status_message = format!("Startup error: {e}"),
        }
    }
    ui.label(
        egui::RichText::new("Start Slipkey automatically after login.")
            .small()
            .weak(),
    );

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // Hook status
    ui.horizontal(|ui| {
        let (color, label) = if state.hook_active {
            (egui::Color32::from_rgb(50, 200, 80), "Active")
        } else {
            (egui::Color32::from_rgb(200, 80, 50), "Inactive")
        };
        ui.colored_label(color, "●");
        ui.label(label);
        ui.label(egui::RichText::new("— keyboard hook status").small().weak());
    });
}
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/ui/general.rs
git commit -m "feat(win-app): General tab"
```

---

## Task 12 — `ui/shortcuts.rs`

**Files:**
- Create: `bins/slipkey-windows/src/ui/shortcuts.rs`

- [ ] **Create `bins/slipkey-windows/src/ui/shortcuts.rs`**

```rust
use eframe::egui;
use std::sync::mpsc;
use imeswitch_windows::{
    config::{save, Config},
    ime::{detect_default_sources, SourceInfo},
};
use crate::app::AppState;
use crate::hook_thread::HookCmd;

pub fn show(ui: &mut egui::Ui, state: &mut AppState, hook_tx: &mpsc::Sender<HookCmd>) {
    // Leader key
    ui.horizontal(|ui| {
        ui.label("Leader key:");
        let mut leader = state
            .config
            .leader
            .clone()
            .unwrap_or_else(|| ";".to_string());
        let te = ui.add(
            egui::TextEdit::singleline(&mut leader)
                .desired_width(32.0)
                .font(egui::TextStyle::Monospace),
        );
        if te.changed() {
            state.config.leader = Some(
                leader
                    .chars()
                    .next()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| ";".to_string()),
            );
        }
        ui.label(
            egui::RichText::new("Type this before a prefix like ;en")
                .small()
                .weak(),
        );
    });

    ui.add_space(8.0);

    // Table header
    ui.horizontal(|ui| {
        ui.add_sized([90.0, 16.0], egui::Label::new(egui::RichText::new("Language").small().strong()));
        ui.add_sized([58.0, 16.0], egui::Label::new(egui::RichText::new("Prefix").small().strong()));
        ui.label(egui::RichText::new("Input source").small().strong());
    });
    ui.separator();

    // Mapping rows
    let mappings = state
        .config
        .mappings
        .get_or_insert_with(|| Config::default().mappings.unwrap_or_default());

    for mapping in mappings.iter_mut() {
        ui.horizontal(|ui| {
            let lang_label = match mapping.language.as_str() {
                "en" => "English",
                "ja" => "日本語",
                "zh" => "中文",
                other => other,
            };
            ui.add_sized([90.0, 20.0], egui::Label::new(lang_label));

            ui.add(
                egui::TextEdit::singleline(&mut mapping.prefix)
                    .desired_width(50.0),
            );

            let current_label =
                source_display(&mapping.source, &state.detected_sources);
            egui::ComboBox::from_id_source(&mapping.language)
                .width(200.0)
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for src in state
                        .detected_sources
                        .iter()
                        .filter(|s| s.language == mapping.language)
                    {
                        let label = format!("{} ({})", src.name, src.id);
                        ui.selectable_value(&mut mapping.source, src.id.clone(), label);
                    }
                    // Always show current value even if absent from detected list
                    if !state.detected_sources.iter().any(|s| s.id == mapping.source) {
                        ui.selectable_value(
                            &mut mapping.source,
                            mapping.source.clone(),
                            &mapping.source,
                        );
                    }
                });
        });
        ui.add_space(2.0);
    }

    ui.separator();
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if !state.status_message.is_empty() {
            ui.label(egui::RichText::new(&state.status_message).small().weak());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Save").clicked() {
                let result = save(&state.config).and_then(|()| {
                    hook_tx
                        .send(HookCmd::Restart(state.config.clone()))
                        .map_err(|e| anyhow::anyhow!("{e}"))
                });
                match result {
                    Ok(()) => {
                        state.status_message = "Saved. Shortcuts are active now.".to_string()
                    }
                    Err(e) => state.status_message = format!("Save failed: {e}"),
                }
            }
            if ui.button("Detect").clicked() {
                state.detected_sources = detect_default_sources();
                state.status_message = String::new();
            }
            if ui.button("Reset").clicked() {
                state.config = Config::default();
                state.status_message =
                    "Defaults restored. Click Save to apply.".to_string();
            }
        });
    });
}

fn source_display(id: &str, sources: &[SourceInfo]) -> String {
    sources
        .iter()
        .find(|s| s.id == id)
        .map(|s| format!("{} ({})", s.name, s.id))
        .unwrap_or_else(|| id.to_string())
}
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/ui/shortcuts.rs
git commit -m "feat(win-app): Shortcuts tab"
```

---

## Task 13 — `ui/mod.rs` (SettingsWindow)

**Files:**
- Create/replace: `bins/slipkey-windows/src/ui/mod.rs`

- [ ] **Replace `bins/slipkey-windows/src/ui/mod.rs`**

```rust
use eframe::egui;
use std::sync::mpsc;
use crate::app::SharedState;
use crate::hook_thread::HookCmd;
use crate::tray::Tray;

pub mod about;
pub mod general;
pub mod shortcuts;

#[derive(PartialEq)]
enum Tab {
    General,
    Shortcuts,
    About,
}

pub struct SettingsWindow {
    state: SharedState,
    hook_tx: mpsc::Sender<HookCmd>,
    tray: Tray,
    tab: Tab,
    icon_texture: Option<egui::TextureHandle>,
}

impl SettingsWindow {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: SharedState,
        hook_tx: mpsc::Sender<HookCmd>,
        tray: Tray,
        icon_rgba: &[u8],
        icon_w: u32,
        icon_h: u32,
    ) -> Self {
        let icon_texture = cc.egui_ctx.load_texture(
            "app_icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon_w as usize, icon_h as usize],
                icon_rgba,
            ),
            egui::TextureOptions::default(),
        );
        Self {
            state,
            hook_tx,
            tray,
            tab: Tab::General,
            icon_texture: Some(icon_texture),
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Hide on close instead of quitting.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Poll tray icon click events.
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { .. } = event {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // Poll tray menu events.
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.tray.open_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if event.id == self.tray.quit_id {
                std::process::exit(0);
            }
        }

        // Repaint periodically to keep tray events responsive.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Tab bar.
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::General, "⚙  General");
                ui.add_space(4.0);
                ui.selectable_value(&mut self.tab, Tab::Shortcuts, "⌨  Shortcuts");
                ui.add_space(4.0);
                ui.selectable_value(&mut self.tab, Tab::About, "ℹ  About");
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::General => {
                    let mut state = self.state.lock().unwrap();
                    general::show(ui, &mut state);
                }
                Tab::Shortcuts => {
                    let mut state = self.state.lock().unwrap();
                    shortcuts::show(ui, &mut state, &self.hook_tx);
                }
                Tab::About => {
                    about::show(ui, self.icon_texture.as_ref());
                }
            }
        });
    }
}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/ui/mod.rs bins/slipkey-windows/src/ui/about.rs bins/slipkey-windows/src/ui/general.rs bins/slipkey-windows/src/ui/shortcuts.rs
git commit -m "feat(win-app): settings window — three tabs (General/Shortcuts/About)"
```

---

## Task 14 — `main.rs` (entry point)

**Files:**
- Replace: `bins/slipkey-windows/src/main.rs`

- [ ] **Replace `bins/slipkey-windows/src/main.rs`**

```rust
// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod hook_thread;
mod startup;
mod tray;
mod ui;

use std::sync::{Arc, Mutex, mpsc};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    // Load state from config file + registry.
    let state: app::SharedState = Arc::new(Mutex::new(app::AppState::load()));

    // Hook command channel (main → hook thread).
    let (hook_tx, hook_rx) = mpsc::channel::<hook_thread::HookCmd>();

    // Start the hook background thread.
    hook_thread::spawn(state.clone(), hook_rx);

    // Decode icon once; reuse for tray, window icon, and about texture.
    let (icon_rgba, icon_w, icon_h) = load_icon();

    // Build system tray icon (must be created on the main thread on Windows).
    let tray = tray::Tray::new(icon_rgba.clone(), icon_w, icon_h);

    // Launch egui window (hidden by default; revealed via tray click).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Slipkey")
            .with_inner_size([500.0, 360.0])
            .with_resizable(false)
            .with_visible(false)
            .with_icon(egui::IconData {
                rgba: icon_rgba.clone(),
                width: icon_w,
                height: icon_h,
            }),
        ..Default::default()
    };

    let state_for_app = state.clone();
    let hook_tx_for_app = hook_tx.clone();
    let icon_for_app = icon_rgba.clone();

    eframe::run_native(
        "Slipkey",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::SettingsWindow::new(
                cc,
                state_for_app,
                hook_tx_for_app,
                tray,
                &icon_for_app,
                icon_w,
                icon_h,
            )))
        }),
    )
    .expect("eframe failed");
}

fn load_icon() -> (Vec<u8>, u32, u32) {
    let bytes = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(bytes)
        .expect("icon decode failed")
        .into_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}
```

- [ ] **Verify**

```bash
cargo check --workspace --target x86_64-pc-windows-msvc
```

Expected: `Finished` with no errors.

- [ ] **Commit**

```bash
git add bins/slipkey-windows/src/main.rs
git commit -m "feat(win-app): main entry point — tray + egui event loop"
```

---

## Task 15 — Update README

**Files:**
- Modify: `README.md`

- [ ] **Update the Windows section** — replace the build/run block and architecture table

In `README.md` find the Windows section and replace with:

```markdown
## Windows

### Requirements

- Windows 10/11 x64, Rust toolchain with `x86_64-pc-windows-msvc`

### Build

```bash
cargo build --release -p slipkey-windows --target x86_64-pc-windows-msvc
```

Copy `target/x86_64-pc-windows-msvc/release/Slipkey.exe` to the machine and run it.

### Usage

1. Double-click `Slipkey.exe` — a tray icon appears in the notification area
2. Right-click the tray icon → **Open Settings** to configure shortcuts
3. Go to **General** tab → enable **Launch at login**

### Config

`%APPDATA%\imeswitch\config.toml` — editable via the **Shortcuts** tab or manually (same schema as macOS).

### Uninstall

1. Quit via the tray menu
2. Delete `Slipkey.exe`
3. Delete `%APPDATA%\imeswitch\`
4. Optionally remove `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Slipkey` from the registry
```

And update the architecture table to replace `imeswitchd` with `slipkey-windows`:

```markdown
bins/
  slipkey-app/          macOS native app (Swift, SwiftPM)
  slipkey-windows/      Windows native app (Rust, egui)
    src/
      hook_thread.rs    WH_KEYBOARD_LL + PeekMessageW loop
      startup.rs        Registry launch-at-login
      tray.rs           System tray icon + menu
      ui/               egui settings window (General/Shortcuts/About)
```

- [ ] **Commit**

```bash
git add README.md
git commit -m "docs: update Windows section for Slipkey.exe (egui tray app)"
```

---

## Self-Review

**Spec coverage:**
- ✅ TSF composition fix — Task 1 + Task 4
- ✅ Input source enrichment — Task 2
- ✅ Config save — Task 3
- ✅ Launch at login — Task 7
- ✅ System tray — Task 9
- ✅ Settings window (General/Shortcuts/About) — Tasks 10-13
- ✅ Remove imeswitchd — Task 5
- ✅ README — Task 15

**Type consistency check:**
- `HookCmd::Restart(Config)` used in Tasks 8, 12 ✅
- `SharedState = Arc<Mutex<AppState>>` used in Tasks 6, 8, 13, 14 ✅
- `EventHook::install_with_mappings(u32, I, Arc<Mutex<ImeSwitcher>>)` defined in Task 4, used in Task 8 ✅
- `EventHook::update_config(&self, &Mapping)` defined in Task 4, used in Task 8 ✅
- `detect_default_sources() -> Vec<SourceInfo>` defined in Task 2, used in Tasks 6, 12 ✅
- `save(config: &Config) -> anyhow::Result<()>` defined in Task 3, used in Task 12 ✅
- `SourceInfo { id, name, language }` defined in Task 2, used in Tasks 6, 12 ✅

**Windows-only guards:** all registry calls in `startup.rs` are wrapped in `#[cfg(target_os = "windows")]` / `#[cfg(not(...))]`. ✅

**No placeholders:** all steps contain complete code. ✅
