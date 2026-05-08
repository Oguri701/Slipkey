//! Keyboard layout discovery helpers.
//!
//! Used by the settings UI "Detect" button to enumerate installed CJK layouts
//! and by diagnostics to report the currently active layout.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
}

#[cfg(target_os = "windows")]
pub fn current_source_id() -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    unsafe {
        let foreground = GetForegroundWindow();
        let tid = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, std::ptr::null_mut())
        };
        let hkl = GetKeyboardLayout(tid);
        if hkl.is_null() {
            None
        } else {
            Some(super::layout::format_hkl(hkl))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn current_source_id() -> Option<String> {
    None
}

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
            let id = super::layout::format_hkl(hkl);
            let langid = (hkl as usize & 0xFFFF) as u32;
            SourceInfo {
                id,
                name: locale_language_name(langid),
                language: langid_to_iso(langid),
            }
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn list_all_sources() -> Vec<SourceInfo> {
    Vec::new()
}

/// Returns installed CJK keyboard layouts (Japanese and Chinese only).
/// English does not require HKL detection in the Windows model — `;en`
/// switches the current CJK IME to alphanumeric mode without changing layout.
pub fn detect_default_sources() -> Vec<SourceInfo> {
    list_all_sources()
        .into_iter()
        .filter(|s| matches!(s.language.as_str(), "ja" | "zh"))
        .collect()
}

#[cfg(target_os = "windows")]
fn langid_to_iso(langid: u32) -> String {
    match langid & 0xFFFF {
        0x0409 | 0x0809 | 0x0C09 | 0x1009 | 0x1409 | 0x1809 => "en".to_string(),
        0x0411 => "ja".to_string(),
        0x0412 => "ko".to_string(),
        0x0804 | 0x0404 | 0x0C04 | 0x1404 => "zh".to_string(),
        other => match other & 0x3FF {
            0x09 => "en".to_string(),
            0x11 => "ja".to_string(),
            0x12 => "ko".to_string(),
            0x04 => "zh".to_string(),
            _ => format!("{:04X}", langid),
        },
    }
}

#[cfg(target_os = "windows")]
fn locale_language_name(langid: u32) -> String {
    use windows_sys::Win32::Globalization::GetLocaleInfoW;

    const LOCALE_SENGLISHLANGUAGENAME: u32 = 0x0001_0001;
    let mut buf = vec![0u16; 128];
    let len = unsafe {
        GetLocaleInfoW(
            langid,
            LOCALE_SENGLISHLANGUAGENAME,
            buf.as_mut_ptr(),
            buf.len() as i32,
        )
    };
    if len > 1 {
        String::from_utf16_lossy(&buf[..len as usize - 1])
    } else {
        format!("{:04X}", langid)
    }
}
