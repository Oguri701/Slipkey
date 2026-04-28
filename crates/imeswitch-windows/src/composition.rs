//! Windows IME composition detection via IMM32.

use windows_sys::Win32::UI::Input::Ime::{
    ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext, GCS_COMPSTR,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

pub fn is_composing() -> bool {
    let hwnd = focused_window();
    if hwnd.is_null() {
        return false;
    }

    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_null() {
            return false;
        }
        let bytes = ImmGetCompositionStringW(himc, GCS_COMPSTR, std::ptr::null_mut(), 0);
        ImmReleaseContext(hwnd, himc);
        bytes > 0
    }
}

fn focused_window() -> windows_sys::Win32::Foundation::HWND {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return foreground;
        }
        // GetGUIThreadInfo(0, ...) reports OUR thread, not the foreground
        // app's. Always resolve the foreground thread id explicitly so we
        // see the focused child control (the actual edit field), not the
        // top-level window.
        let tid = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        if tid == 0 {
            return foreground;
        }

        let mut info = std::mem::zeroed::<GUITHREADINFO>();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(tid, &mut info) != 0 && !info.hwndFocus.is_null() {
            info.hwndFocus
        } else {
            foreground
        }
    }
}

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
