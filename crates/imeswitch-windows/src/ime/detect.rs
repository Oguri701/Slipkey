//! Keyboard layout discovery helpers.
//!
//! Used by the settings UI "Detect" button to enumerate installed CJK layouts
//! and by diagnostics to report the currently active layout.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub platform: String,
    pub id: String,
    pub name: String,
    pub raw_language: String,
    pub language: String,
    pub is_selectable: bool,
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
            let language = langid_to_iso(langid);
            SourceInfo {
                platform: "windows".to_string(),
                id,
                name: locale_language_name(langid),
                raw_language: format!("{langid:04X}"),
                language,
                is_selectable: true,
            }
        })
        .filter(|source| is_supported_language(&source.language))
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn list_all_sources() -> Vec<SourceInfo> {
    Vec::new()
}

/// Returns installed keyboard layouts that Slipkey can map to standard
/// language prefixes. The old name is kept because the Windows UI already
/// calls it as its "Detect" action.
pub fn detect_default_sources() -> Vec<SourceInfo> {
    list_all_sources()
}

pub fn is_cjk_langid(langid: u32) -> bool {
    matches!(primary_langid(langid), 0x04 | 0x11 | 0x12)
}

#[cfg(target_os = "windows")]
fn langid_to_iso(langid: u32) -> String {
    match langid & 0xFFFF {
        0x0409 | 0x0809 | 0x0C09 | 0x1009 | 0x1409 | 0x1809 => "en".to_string(),
        0x0411 => "ja".to_string(),
        0x0412 => "ko".to_string(),
        0x0804 | 0x0404 | 0x0C04 | 0x1404 => "zh".to_string(),
        0x040C => "fr".to_string(),
        0x0407 => "de".to_string(),
        0x0C0A | 0x040A => "es".to_string(),
        0x0410 => "it".to_string(),
        0x0419 => "ru".to_string(),
        other => match primary_langid(other) {
            0x09 => "en".to_string(),
            0x11 => "ja".to_string(),
            0x12 => "ko".to_string(),
            0x04 => "zh".to_string(),
            0x0C => "fr".to_string(),
            0x07 => "de".to_string(),
            0x0A => "es".to_string(),
            0x10 => "it".to_string(),
            0x19 => "ru".to_string(),
            _ => format!("{:04X}", langid),
        },
    }
}

fn is_supported_language(language: &str) -> bool {
    matches!(
        language,
        "en" | "ja" | "zh" | "ko" | "fr" | "de" | "es" | "it" | "ru"
    )
}

fn primary_langid(langid: u32) -> u32 {
    langid & 0x3FF
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_langid_uses_primary_language_id() {
        assert!(is_cjk_langid(0x0411)); // Japanese
        assert!(is_cjk_langid(0x0804)); // Chinese Simplified
        assert!(is_cjk_langid(0x0C04)); // Chinese Hong Kong
        assert!(is_cjk_langid(0x0412)); // Korean
        assert!(!is_cjk_langid(0x0409)); // English
    }

    #[test]
    fn common_langids_normalize_to_short_codes() {
        assert_eq!(langid_to_iso(0x0409), "en");
        assert_eq!(langid_to_iso(0x0411), "ja");
        assert_eq!(langid_to_iso(0x0804), "zh");
        assert_eq!(langid_to_iso(0x0412), "ko");
        assert_eq!(langid_to_iso(0x040C), "fr");
        assert_eq!(langid_to_iso(0x0407), "de");
        assert_eq!(langid_to_iso(0x0C0A), "es");
    }
}
