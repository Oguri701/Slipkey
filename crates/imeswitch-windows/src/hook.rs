//! Windows low-level keyboard hook that feeds keydowns into `imeswitch-core`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use imeswitch_core::{Key, Language, StateMachine};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::composition::{is_cjk_ime_active, is_composing};
use crate::ime::{WindowsImeSwitcher, WinMapping};
use crate::keymap::{
    is_leader_vk, key_to_vk_with_leader, leader_vk_for, vk_to_key_with_leader, VK_SEMICOLON,
};

const REPLAY_MAGIC: usize = 0x696d_6573_7769_6e36;
const COMPOSITION_IDLE_THRESHOLD: Duration = Duration::from_millis(500);

static HOOK_STATE: Mutex<Option<HookState>> = Mutex::new(None);

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
    /// then switcher, never hold both simultaneously (see handle_keydown).
    switcher: Arc<Mutex<WindowsImeSwitcher>>,
    possible_composition: bool,
    last_keydown: Option<Instant>,
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
        switcher: Arc<Mutex<WindowsImeSwitcher>>,
    ) -> Result<Self, HookError>
    where
        I: IntoIterator<Item = (Language, String)>,
    {
        {
            let mut guard = HOOK_STATE.lock().unwrap();
            if guard.is_some() {
                return Err(HookError::AlreadyInstalled);
            }
            *guard = Some(HookState {
                sm: StateMachine::from_mappings(mappings),
                switcher,
                possible_composition: false,
                last_keydown: None,
                leader_vk,
            });
        }

        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                std::ptr::null_mut(),
                0,
            )
        };
        if hook.is_null() {
            *HOOK_STATE.lock().unwrap() = None;
            return Err(HookError::InstallFailed);
        }
        Ok(Self { hook })
    }

    /// Update the state machine and leader key without reinstalling the hook.
    /// Call this after the shared `Arc<Mutex<WindowsImeSwitcher>>` has already been
    /// updated with the new mapping.
    pub fn update_config(&self, mapping: &WinMapping) {
        if let Some(state) = HOOK_STATE.lock().unwrap().as_mut() {
            state.sm = StateMachine::from_mappings(mapping.trigger_mappings());
            state.leader_vk = leader_vk_for(mapping.leader()).unwrap_or(VK_SEMICOLON);
        }
    }
}

impl Drop for EventHook {
    fn drop(&mut self) {
        if !self.hook.is_null() {
            unsafe {
                UnhookWindowsHookEx(self.hook);
            }
        }
        *HOOK_STATE.lock().unwrap() = None;
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

    match handle_keydown(kb.vkCode, kb.scanCode, kb.flags) {
        Ok(true) => 1,
        Ok(false) => CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param),
        Err(err) => {
            log::error!("keyboard hook error: {}", err);
            CallNextHookEx(std::ptr::null_mut(), code, w_param, l_param)
        }
    }
}

fn handle_keydown(vk: u32, scan_code: u32, flags: u32) -> Result<bool, HookError> {
    let composing = is_composing();
    let has_mod = has_blocking_modifier();
    let now = Instant::now();

    // Clone the Arc while holding the HOOK_STATE lock, then release the lock
    // before calling switch_to. This prevents deadlock when update_config
    // locks switcher first and then HOOK_STATE.
    let (response, leader_vk, switcher_arc) = {
        let mut guard = HOOK_STATE.lock().unwrap();
        let state = guard.as_mut().ok_or(HookError::StateUnavailable)?;
        let leader_vk = state.leader_vk;
        let recently_typed = state
            .last_keydown
            .map(|last| now.duration_since(last) < COMPOSITION_IDLE_THRESHOLD)
            .unwrap_or(false);
        let is_leader_key = is_leader_vk(vk, leader_vk);
        let should_defer = should_defer_to_ime(
            state.sm.is_idle(),
            is_leader_key,
            composing,
            state.possible_composition,
            recently_typed,
            is_composition_ending_key(vk, leader_vk),
        );

        log::debug!(
            "kd vk={:#04x} scan={:#04x} flags={:#04x} composing={} possible={} recent={} leader={} mod={} defer={}",
            vk,
            scan_code,
            flags,
            composing,
            state.possible_composition,
            recently_typed,
            is_leader_key,
            has_mod,
            should_defer,
        );

        if should_defer {
            state.last_keydown = Some(now);
            update_possible_composition(state, true, vk, false);
            return Ok(false);
        }

        let key = if has_mod {
            Key::Other
        } else {
            vk_to_key_with_leader(vk, leader_vk)
        };
        let response = state.sm.on_keydown(key);
        state.last_keydown = Some(now);
        update_possible_composition(state, composing, vk, response.switch.is_some());

        log::debug!(
            "  -> key={:?} suppress={} replay={:?} switch={:?}",
            key,
            response.suppress,
            response.replay,
            response.switch,
        );

        let switcher_arc = state.switcher.clone();
        (response, leader_vk, switcher_arc)
    };

    if let Some(ref lang) = response.switch {
        if let Err(error) = switcher_arc.lock().unwrap().switch_to(lang) {
            log::error!("switch failed: {}", error);
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
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ]
    .iter()
    .any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } < 0)
}

fn update_possible_composition(state: &mut HookState, composing: bool, vk: u32, did_switch: bool) {
    if did_switch || is_composition_ending_key(vk, state.leader_vk) {
        state.possible_composition = false;
    } else if composing || vk_to_key_with_leader(vk, state.leader_vk) == Key::Other {
        state.possible_composition = true;
    } else if is_cjk_ime_active()
        && matches!(vk_to_key_with_leader(vk, state.leader_vk), Key::AlphaNum(_))
    {
        // TSF-based IMEs (Microsoft Pinyin/Japanese) don't report via IMM32.
        // Treat alphanum presses while a CJK layout is active as potential
        // composition input, mirroring the macOS idle fallback.
        state.possible_composition = true;
    }
}

fn should_defer_to_ime(
    is_idle: bool,
    is_leader_key: bool,
    is_composing: bool,
    possible_composition: bool,
    recently_typed: bool,
    is_composition_ending_key: bool,
) -> bool {
    if !is_idle || is_leader_key {
        return false;
    }

    is_composing || (possible_composition && recently_typed && !is_composition_ending_key)
}

fn is_composition_ending_key(vk: u32, _leader_vk: u32) -> bool {
    matches!(
        vk,
        x if x == VK_SPACE as u32
            || x == VK_BACK as u32
            || x == VK_DELETE as u32
            || x == VK_ESCAPE as u32
            || x == 0x0D
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leader_key_starts_trigger_even_during_composition() {
        assert!(!should_defer_to_ime(true, true, true, true, true, false));
    }

    #[test]
    fn idle_non_leader_defers_to_active_composition() {
        assert!(should_defer_to_ime(true, false, true, false, false, false));
    }

    #[test]
    fn stale_possible_composition_does_not_block_new_trigger() {
        assert!(!should_defer_to_ime(true, false, false, true, false, false));
    }

    #[test]
    fn active_trigger_sequence_does_not_defer_to_ime() {
        assert!(!should_defer_to_ime(false, false, true, true, true, false));
    }
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
