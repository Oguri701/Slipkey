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
