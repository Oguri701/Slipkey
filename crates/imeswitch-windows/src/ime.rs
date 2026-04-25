//! Windows keyboard layout / IME switching.
//!
//! Public types mirror the macOS crate so the daemon glue can stay thin.

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
pub struct Mapping {
    pub en: String,
    pub ja: String,
    pub zh: String,
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            en: "00000409".to_string(),
            ja: "00000411".to_string(),
            zh: "00000804".to_string(),
        }
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

    pub fn switch_to(&self, lang: Language) -> Result<(), SwitchError> {
        let id = match lang {
            Language::En => &self.mapping.en,
            Language::Ja => &self.mapping.ja,
            Language::Zh => &self.mapping.zh,
        };
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
        .map(|hkl| SourceInfo {
            id: format_hkl(hkl),
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
pub fn list_all_sources() -> Vec<SourceInfo> {
    Vec::new()
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
        assert_eq!(m.en, "00000409");
        assert_eq!(m.ja, "00000411");
        assert_eq!(m.zh, "00000804");
    }
}
