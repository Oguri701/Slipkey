//! Windows keyboard layout (HKL) management.
//!
//! Responsible for enumerating installed layouts, loading them, and issuing
//! `WM_INPUTLANGCHANGEREQUEST` messages to switch the active input method.

use super::SwitchError;

/// Returns the focused child window (the actual edit field), falling back to
/// the foreground window. Used as the target for layout-change messages and
/// IME context operations.
#[cfg(target_os = "windows")]
pub fn focused_window() -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return foreground;
        }
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

#[cfg(not(target_os = "windows"))]
pub fn focused_window() -> *mut std::ffi::c_void {
    std::ptr::null_mut()
}

/// Find an already-loaded keyboard layout by its HKL ID string.
#[cfg(target_os = "windows")]
pub fn find_installed_layout(
    id: &str,
) -> Option<windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutList;

    let normalized = id.trim().to_ascii_uppercase();
    let wanted = u32::from_str_radix(&normalized, 16).ok();
    let wanted_langid = wanted.map(|v| v & 0xFFFF);
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    if count <= 0 {
        return None;
    }
    let mut layouts = vec![std::ptr::null_mut(); count as usize];
    let actual = unsafe { GetKeyboardLayoutList(count, layouts.as_mut_ptr()) };
    let layouts: Vec<_> = layouts.into_iter().take(actual.max(0) as usize).collect();

    for &hkl in &layouts {
        if format_hkl(hkl).eq_ignore_ascii_case(&normalized) {
            return Some(hkl);
        }
    }

    let wanted_langid = wanted_langid?;
    layouts
        .into_iter()
        .find(|&hkl| ((hkl as usize) as u32 & 0xFFFF) == wanted_langid)
}

/// Load or find a keyboard layout, returning its HKL handle.
#[cfg(target_os = "windows")]
pub fn load_or_find_layout(
    id: &str,
) -> Result<windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL, SwitchError> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{LoadKeyboardLayoutW, KLF_ACTIVATE};

    if let Some(hkl) = find_installed_layout(id) {
        return Ok(hkl);
    }
    let wide = wide_null(id);
    let hkl = unsafe { LoadKeyboardLayoutW(wide.as_ptr(), KLF_ACTIVATE) };
    if hkl.is_null() {
        Err(SwitchError::NotInstalled(id.to_string()))
    } else {
        Ok(hkl)
    }
}

/// Switch the focused window's keyboard layout synchronously via `SendMessageW`.
///
/// After this call returns, the focused window has processed
/// `WM_INPUTLANGCHANGEREQUEST` and its IME context reflects the new layout.
/// It is therefore safe to call `set_ime_native_mode` immediately after.
#[cfg(target_os = "windows")]
pub fn switch_layout_sync(
    hwnd: windows_sys::Win32::Foundation::HWND,
    hkl: windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL,
) -> Result<(), SwitchError> {
    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_INPUTLANGCHANGEREQUEST};

    if hwnd.is_null() {
        log::warn!(
            "switch_layout_sync: focused window is null, skipping synchronous layout message"
        );
        return Ok(());
    }
    unsafe { SendMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0 as WPARAM, hkl as LPARAM) };
    Ok(())
}

/// Broadcast a layout change to all top-level windows asynchronously.
/// This updates the taskbar IME indicator and notifies other applications.
#[cfg(target_os = "windows")]
pub fn broadcast_layout_change(hkl: windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL) {
    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, HWND_BROADCAST, WM_INPUTLANGCHANGEREQUEST,
    };
    unsafe {
        PostMessageW(
            HWND_BROADCAST,
            WM_INPUTLANGCHANGEREQUEST,
            0 as WPARAM,
            hkl as LPARAM,
        );
    }
}

#[cfg(target_os = "windows")]
pub fn format_hkl(hkl: windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL) -> String {
    format!("{:08X}", (hkl as usize) & 0xFFFF_FFFF)
}

#[cfg(target_os = "windows")]
pub fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
