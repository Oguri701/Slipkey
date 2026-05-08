//! Windows IME conversion mode control.
//!
//! Modern Windows IMEs mix IMM32 and TSF behavior, so mode setting needs more
//! than one path:
//! 1. IMM32 open/conversion status for Chinese on/off and legacy IME contexts.
//! 2. The per-window default IME window for apps that route mode changes there.
//! 3. DBE virtual keys as absolute-mode hints for Microsoft Japanese IME.

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Input::Ime::{
    ImmGetContext, ImmGetConversionStatus, ImmGetDefaultIMEWnd, ImmReleaseContext,
    ImmSetConversionStatus, ImmSetOpenStatus, IMC_SETCONVERSIONMODE, IMC_SETOPENSTATUS,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, VK_DBE_ALPHANUMERIC, VK_DBE_HIRAGANA, VK_NONCONVERT,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SendMessageW, WM_IME_CONTROL,
};

const IME_CMODE_ALPHANUMERIC: u32 = 0x0000;
const IME_CMODE_NATIVE: u32 = 0x0001;
const IME_CMODE_FULLSHAPE: u32 = 0x0008;
const IME_CMODE_ROMAN: u32 = 0x0010;
const SCANCODE_CAPSLOCK_EISU: u16 = 0x3a;

// Must match hook.rs so the hook ignores our simulated DBE keys.
const REPLAY_MAGIC: usize = 0x696d_6573_7769_6e36;

/// Set the focused window's IME to native CJK input mode.
#[cfg(target_os = "windows")]
pub fn set_ime_native_mode(hwnd: windows_sys::Win32::Foundation::HWND) {
    set_ime_native_mode_for_language(hwnd, "ja");
}

#[cfg(target_os = "windows")]
pub fn set_ime_native_mode_for_language(
    hwnd: windows_sys::Win32::Foundation::HWND,
    language: &str,
) {
    let mode = native_conversion_mode(language);
    let target = usable_hwnd(hwnd);
    let imm32_ok = imm32_set_mode(target, true, mode);
    let window_ok = ime_window_set_mode(target, true, mode);
    log::debug!(
        "set_ime_native_mode_for_language({language}): imm32={imm32_ok} ime_window={window_ok}"
    );
    if language == "ja" {
        send_virtual_key(VK_DBE_HIRAGANA);
    }
}

/// Set the focused window's IME to alphanumeric/Latin input mode.
#[cfg(target_os = "windows")]
pub fn set_ime_alphanumeric_mode(hwnd: windows_sys::Win32::Foundation::HWND) {
    let target = usable_hwnd(hwnd);
    let language = active_keyboard_language(target);
    let keep_ime_open = keep_ime_open_for_alphanumeric(language);

    let imm32_ok = imm32_set_mode(target, keep_ime_open, IME_CMODE_ALPHANUMERIC);
    let window_ok = ime_window_set_mode(target, keep_ime_open, IME_CMODE_ALPHANUMERIC);
    log::debug!(
        "set_ime_alphanumeric_mode({:?}): keep_open={} imm32={} ime_window={}",
        language,
        keep_ime_open,
        imm32_ok,
        window_ok
    );
    if language.as_deref() == Some("ja") {
        send_scan_code(SCANCODE_CAPSLOCK_EISU);
        std::thread::sleep(std::time::Duration::from_millis(10));
        send_virtual_key(VK_NONCONVERT);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    send_virtual_key(VK_DBE_ALPHANUMERIC);
}

/// Attempt to set open/conversion mode via IMM32.
/// Uses `GetForegroundWindow()` for reliable cross-thread access.
#[cfg(target_os = "windows")]
fn imm32_set_mode(hwnd: windows_sys::Win32::Foundation::HWND, open: bool, conversion: u32) -> bool {
    unsafe {
        if hwnd.is_null() {
            log::debug!("imm32_set_mode: no foreground window");
            return false;
        }
        let himc = ImmGetContext(hwnd);
        if himc.is_null() {
            log::debug!("imm32_set_mode: ImmGetContext returned null");
            return false;
        }

        let mut old_conversion: u32 = 0;
        let mut sentence: u32 = 0;
        ImmGetConversionStatus(himc, &mut old_conversion, &mut sentence);
        let open_ok = ImmSetOpenStatus(himc, open as i32) != 0;
        let conversion_ok = ImmSetConversionStatus(himc, conversion, sentence) != 0;
        ImmReleaseContext(hwnd, himc);

        log::debug!(
            "imm32_set_mode: open={} conv {:#010x}->{:#010x} open_ok={} conversion_ok={}",
            open,
            old_conversion,
            conversion,
            open_ok,
            conversion_ok
        );
        open_ok || conversion_ok
    }
}

#[cfg(target_os = "windows")]
fn ime_window_set_mode(
    hwnd: windows_sys::Win32::Foundation::HWND,
    open: bool,
    conversion: u32,
) -> bool {
    unsafe {
        if hwnd.is_null() {
            return false;
        }
        let ime = ImmGetDefaultIMEWnd(hwnd);
        if ime.is_null() {
            return false;
        }

        let open_result = SendMessageW(
            ime,
            WM_IME_CONTROL,
            IMC_SETOPENSTATUS as usize,
            open as isize,
        );
        let conversion_result = SendMessageW(
            ime,
            WM_IME_CONTROL,
            IMC_SETCONVERSIONMODE as usize,
            conversion as isize,
        );
        log::debug!(
            "ime_window_set_mode: open={} conv={:#010x} open_result={} conversion_result={}",
            open,
            conversion,
            open_result,
            conversion_result
        );
        open_result != 0 || conversion_result != 0
    }
}

fn native_conversion_mode(language: &str) -> u32 {
    match language {
        "ja" => IME_CMODE_NATIVE | IME_CMODE_FULLSHAPE | IME_CMODE_ROMAN,
        _ => IME_CMODE_NATIVE,
    }
}

fn keep_ime_open_for_alphanumeric(language: Option<&str>) -> bool {
    !matches!(language, Some("ja") | Some("zh"))
}

#[cfg(target_os = "windows")]
fn usable_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) -> windows_sys::Win32::Foundation::HWND {
    if hwnd.is_null() {
        unsafe { GetForegroundWindow() }
    } else {
        hwnd
    }
}

#[cfg(target_os = "windows")]
fn active_keyboard_language(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<&'static str> {
    unsafe {
        let thread_id = if hwnd.is_null() {
            0
        } else {
            GetWindowThreadProcessId(hwnd, std::ptr::null_mut())
        };
        let hkl = GetKeyboardLayout(thread_id);
        let langid = (hkl as usize) & 0xFFFF;
        match langid {
            0x0411 => Some("ja"),
            0x0804 | 0x0404 | 0x0C04 | 0x1404 => Some("zh"),
            0x0412 => Some("ko"),
            _ => None,
        }
    }
}

/// Simulate a DBE virtual-key press+release via SendInput.
/// dwExtraInfo is set to REPLAY_MAGIC so our WH_KEYBOARD_LL hook ignores it.
#[cfg(target_os = "windows")]
fn send_virtual_key(vk: u16) {
    let mut inputs = [dbe_input(vk, 0), dbe_input(vk, KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(2, inputs.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "windows")]
fn send_scan_code(scan_code: u16) {
    let mut inputs = [
        scan_code_input(scan_code, KEYEVENTF_SCANCODE),
        scan_code_input(scan_code, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP),
    ];
    unsafe {
        SendInput(2, inputs.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "windows")]
fn dbe_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: REPLAY_MAGIC,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn scan_code_input(scan_code: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: REPLAY_MAGIC,
            },
        },
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_ime_native_mode(_hwnd: *mut std::ffi::c_void) {}

#[cfg(not(target_os = "windows"))]
pub fn set_ime_native_mode_for_language(_hwnd: *mut std::ffi::c_void, _language: &str) {}

#[cfg(not(target_os = "windows"))]
pub fn set_ime_alphanumeric_mode(_hwnd: *mut std::ffi::c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_native_mode_does_not_force_full_shape() {
        assert_eq!(native_conversion_mode("zh"), IME_CMODE_NATIVE);
    }

    #[test]
    fn japanese_native_mode_forces_hiragana_style_bits() {
        assert_eq!(
            native_conversion_mode("ja"),
            IME_CMODE_NATIVE | IME_CMODE_FULLSHAPE | IME_CMODE_ROMAN
        );
    }

    #[test]
    fn japanese_and_chinese_alphanumeric_close_ime_without_changing_hkl() {
        assert!(!keep_ime_open_for_alphanumeric(Some("ja")));
        assert!(!keep_ime_open_for_alphanumeric(Some("zh")));
        assert!(keep_ime_open_for_alphanumeric(None));
    }
}
