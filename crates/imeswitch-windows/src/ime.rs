//! Windows keyboard layout / IME switching.
//!
//! Public types mirror the macOS crate so the daemon glue can stay thin.

use std::collections::HashMap;

use imeswitch_core::Language;

#[derive(Debug)]
pub enum SwitchError {
    NotInstalled(String),
    SelectFailed(String),
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchError::NotInstalled(id) => write!(
                f,
                "keyboard layout '{}' not installed or LoadKeyboardLayoutW failed",
                id
            ),
            SwitchError::SelectFailed(id) => {
                write!(
                    f,
                    "WM_INPUTLANGCHANGEREQUEST failed for keyboard layout '{}'",
                    id
                )
            }
        }
    }
}

impl std::error::Error for SwitchError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingEntry {
    pub language: Language,
    pub prefix: String,
    pub source: String,
}

pub const DEFAULT_LEADER: char = ';';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    leader: char,
    entries: Vec<MappingEntry>,
    sources: HashMap<Language, String>,
}

impl Default for Mapping {
    fn default() -> Self {
        Self::new(vec![
            MappingEntry {
                language: Language::from("en"),
                prefix: "en".to_string(),
                source: "00000409".to_string(),
            },
            MappingEntry {
                language: Language::from("ja"),
                prefix: "ja".to_string(),
                source: "00000411".to_string(),
            },
            MappingEntry {
                language: Language::from("zh"),
                prefix: "zh".to_string(),
                source: "00000804".to_string(),
            },
        ])
    }
}

impl Mapping {
    pub fn new(entries: Vec<MappingEntry>) -> Self {
        Self::with_leader(DEFAULT_LEADER, entries)
    }

    pub fn with_leader(leader: char, entries: Vec<MappingEntry>) -> Self {
        let sources = entries
            .iter()
            .map(|entry| (entry.language.clone(), entry.source.clone()))
            .collect();
        Self {
            leader,
            entries,
            sources,
        }
    }

    pub fn leader(&self) -> char {
        self.leader
    }

    pub fn set_leader(&mut self, leader: char) {
        self.leader = leader;
    }

    pub fn entries(&self) -> &[MappingEntry] {
        &self.entries
    }

    pub fn source_for(&self, language: &Language) -> Option<&str> {
        self.sources.get(language).map(String::as_str)
    }

    pub fn trigger_mappings(&self) -> Vec<(Language, String)> {
        self.entries
            .iter()
            .filter(|entry| !entry.prefix.is_empty())
            .map(|entry| (entry.language.clone(), entry.prefix.clone()))
            .collect()
    }

    pub fn describe(&self) -> String {
        let body = self
            .entries
            .iter()
            .map(|entry| format!("{}:{}={}", entry.language, entry.prefix, entry.source))
            .collect::<Vec<_>>()
            .join(" ");
        format!("leader='{}' {}", self.leader, body)
    }
}

pub struct ImeSwitcher {
    mapping: Mapping,
}

impl ImeSwitcher {
    pub fn new() -> Self {
        Self {
            mapping: Mapping::default(),
        }
    }

    pub fn with_mapping(mapping: Mapping) -> Self {
        Self { mapping }
    }

    pub fn switch_to(&self, lang: &Language) -> Result<(), SwitchError> {
        let id = self
            .mapping
            .source_for(lang)
            .ok_or_else(|| SwitchError::NotInstalled(lang.to_string()))?;
        select_layout(id)
    }
}

impl Default for ImeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

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
        // GetKeyboardLayout(0) returns the daemon's own thread layout, which
        // is useless for diagnostics. Resolve the foreground thread id and
        // ask for THAT layout so the before/after log lines match what the
        // user actually sees in their focused app.
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
            Some(format_hkl(hkl))
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn current_source_id() -> Option<String> {
    None
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

#[cfg(not(target_os = "windows"))]
pub fn list_all_sources() -> Vec<SourceInfo> {
    Vec::new()
}

/// Returns installed keyboard layouts whose language matches en, ja, or zh.
/// Used by the Detect button in the settings window.
pub fn detect_default_sources() -> Vec<SourceInfo> {
    list_all_sources()
        .into_iter()
        .filter(|s| matches!(s.language.as_str(), "en" | "ja" | "zh"))
        .collect()
}

#[cfg(target_os = "windows")]
fn select_layout(id: &str) -> Result<(), SwitchError> {
    use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{LoadKeyboardLayoutW, KLF_ACTIVATE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostMessageW, HWND_BROADCAST, WM_INPUTLANGCHANGEREQUEST,
    };

    let wide = wide_null(id);
    let hkl = unsafe { LoadKeyboardLayoutW(wide.as_ptr(), KLF_ACTIVATE) };
    if hkl.is_null() {
        return Err(SwitchError::NotInstalled(id.to_string()));
    }

    let ok = unsafe {
        PostMessageW(
            HWND_BROADCAST,
            WM_INPUTLANGCHANGEREQUEST,
            0 as WPARAM,
            hkl as LPARAM,
        )
    };
    if ok == 0 {
        return Err(SwitchError::SelectFailed(id.to_string()));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn select_layout(_id: &str) -> Result<(), SwitchError> {
    Err(SwitchError::SelectFailed("windows-only".to_string()))
}

#[cfg(target_os = "windows")]
fn format_hkl(hkl: windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL) -> String {
    format!("{:08X}", (hkl as usize) & 0xFFFF_FFFF)
}

#[cfg(target_os = "windows")]
fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_standard_win64_layout_ids() {
        let m = Mapping::default();
        assert_eq!(m.source_for(&Language::from("en")), Some("00000409"));
        assert_eq!(m.source_for(&Language::from("ja")), Some("00000411"));
        assert_eq!(m.source_for(&Language::from("zh")), Some("00000804"));
    }

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
}
