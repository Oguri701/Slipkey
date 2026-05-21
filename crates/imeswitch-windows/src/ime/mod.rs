//! Windows IME switching: HKL management + conversion mode control.
//!
//! Unlike macOS (where each language maps to a distinct input source),
//! Windows CJK IMEs are modal: the same HKL handles both native (CJK)
//! and alphanumeric (Latin) input, toggled via Shift (Chinese) or
//! CapsLock (Japanese). This module expresses that two-axis model
//! explicitly instead of the macOS one-source-per-language approach.

pub mod detect;
pub mod layout;
pub mod tsf_dispatch;

use imeswitch_core::Language;

pub use detect::{current_source_id, detect_default_sources, list_all_sources, SourceInfo};

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
            SwitchError::SelectFailed(id) => write!(
                f,
                "WM_INPUTLANGCHANGEREQUEST failed for keyboard layout '{}'",
                id
            ),
        }
    }
}

impl std::error::Error for SwitchError {}

pub const DEFAULT_LEADER: char = ';';

/// The target input mode for a Windows IME entry.
///
/// Windows CJK IMEs maintain internal mode state (Chinese/alphanumeric,
/// Hiragana/Roman). Switching the HKL alone is not sufficient — the mode
/// must be set explicitly to land in the correct input state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinImeMode {
    /// CJK native input (Chinese characters, Hiragana, etc.).
    /// Applied after switching to the target HKL via IMM32.
    Native,
    /// Latin/alphanumeric input within the current active IME.
    /// The HKL is NOT changed — only the IME's internal mode is toggled.
    Alphanumeric,
    /// Switch the HKL only, without touching the conversion mode.
    /// Used for non-CJK, non-English languages (French, German, etc.)
    /// that have distinct keyboard layouts but no IME conversion modes.
    LayoutOnly,
}

impl WinImeMode {
    /// Derive the target mode from a two-letter ISO language code.
    pub fn for_language(lang: &str) -> Self {
        match lang {
            "en" => Self::Alphanumeric,
            "zh" | "ja" | "ko" => Self::Native,
            _ => Self::LayoutOnly,
        }
    }
}

/// A single shortcut mapping in the Windows model.
///
/// Unlike the macOS model (language → source ID), each Windows entry carries
/// both an optional HKL ID and an explicit IME mode, reflecting that the
/// HKL switch and the conversion mode change are two distinct operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinEntry {
    pub language: Language,
    pub prefix: String,
    /// HKL identifier (e.g. "00000804"). `None` means no layout switch
    /// is performed — only the IME conversion mode is changed (used for `;en`).
    pub hkl_id: Option<String>,
    /// Target IME mode after the entry is activated.
    pub mode: WinImeMode,
}

/// The complete Windows-side shortcut configuration: a leader key and
/// a list of `WinEntry` mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinMapping {
    leader: char,
    entries: Vec<WinEntry>,
}

impl Default for WinMapping {
    fn default() -> Self {
        Self::with_leader(
            DEFAULT_LEADER,
            vec![
                WinEntry {
                    language: Language::from("en"),
                    prefix: "en".to_string(),
                    hkl_id: None,
                    mode: WinImeMode::Alphanumeric,
                },
                WinEntry {
                    language: Language::from("zh"),
                    prefix: "zh".to_string(),
                    hkl_id: Some("00000804".to_string()),
                    mode: WinImeMode::Native,
                },
                WinEntry {
                    language: Language::from("ja"),
                    prefix: "ja".to_string(),
                    hkl_id: Some("00000411".to_string()),
                    mode: WinImeMode::Native,
                },
            ],
        )
    }
}

impl WinMapping {
    pub fn with_leader(leader: char, entries: Vec<WinEntry>) -> Self {
        Self { leader, entries }
    }

    pub fn leader(&self) -> char {
        self.leader
    }

    pub fn set_leader(&mut self, leader: char) {
        self.leader = leader;
    }

    pub fn entries(&self) -> &[WinEntry] {
        &self.entries
    }

    /// Returns `(language, prefix)` pairs for the core state machine.
    pub fn trigger_mappings(&self) -> Vec<(Language, String)> {
        self.entries
            .iter()
            .filter(|e| !e.prefix.is_empty())
            .map(|e| (e.language.clone(), e.prefix.clone()))
            .collect()
    }

    pub fn entry_for(&self, lang: &Language) -> Option<&WinEntry> {
        self.entries.iter().find(|e| &e.language == lang)
    }

    pub fn describe(&self) -> String {
        let body = self
            .entries
            .iter()
            .map(|e| {
                let hkl = e.hkl_id.as_deref().unwrap_or("mode-only");
                format!("{}:{}/{:?}", e.prefix, hkl, e.mode)
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("leader='{}' [{}]", self.leader, body)
    }
}

/// Performs Windows IME switching.
///
/// - `Native` (zh, ja, ko): switches the active HKL then forces the IME into
///   native/CJK input mode via IMM32.
/// - `Alphanumeric` (en): does NOT change the HKL — only sets the current IME
///   to its Latin/alphanumeric input mode.
/// - `LayoutOnly` (fr, de, …): switches the HKL only, without changing the
///   conversion mode (non-CJK keyboards have no IME conversion states).
pub struct WindowsImeSwitcher {
    mapping: WinMapping,
}

impl WindowsImeSwitcher {
    pub fn new() -> Self {
        Self {
            mapping: WinMapping::default(),
        }
    }

    pub fn with_mapping(mapping: WinMapping) -> Self {
        Self { mapping }
    }

    pub fn mapping(&self) -> &WinMapping {
        &self.mapping
    }

    pub fn switch_to(&self, lang: &Language) -> Result<(), SwitchError> {
        let entry = self
            .mapping
            .entry_for(lang)
            .ok_or_else(|| SwitchError::NotInstalled(lang.to_string()))?;
        switch_entry(entry)
    }
}

impl Default for WindowsImeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
fn switch_entry(entry: &WinEntry) -> Result<(), SwitchError> {
    // Step 1: HKL switch (stable, unchanged from before).
    if let Some(hkl_id) = entry.hkl_id.as_deref() {
        let hwnd = layout::focused_window();
        let hkl = layout::load_or_find_layout(hkl_id)?;
        layout::switch_layout_sync(hwnd, hkl)?;
        layout::broadcast_layout_change(hkl);
    }

    // Step 2: TSF Compartment write (Native / Alphanumeric only).
    if let Some(target) = tsf_dispatch::TsfTarget::for_mode(entry.mode, entry.language.as_str()) {
        if let Some(dispatcher) = tsf_dispatch::global() {
            if let Err(e) = dispatcher.dispatch(target) {
                // Silent by design (decision D6): log and move on.
                log::warn!(
                    "TSF dispatch failed for lang={} mode={:?}: {:?}",
                    entry.language.as_str(),
                    entry.mode,
                    e
                );
            }
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn switch_entry(_entry: &WinEntry) -> Result<(), SwitchError> {
    Err(SwitchError::SelectFailed("windows-only".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_correct_hkl_and_mode() {
        let m = WinMapping::default();
        let en = m.entry_for(&Language::from("en")).unwrap();
        assert_eq!(en.hkl_id, None);
        assert_eq!(en.mode, WinImeMode::Alphanumeric);
        let zh = m.entry_for(&Language::from("zh")).unwrap();
        assert_eq!(zh.hkl_id.as_deref(), Some("00000804"));
        assert_eq!(zh.mode, WinImeMode::Native);
        let ja = m.entry_for(&Language::from("ja")).unwrap();
        assert_eq!(ja.hkl_id.as_deref(), Some("00000411"));
        assert_eq!(ja.mode, WinImeMode::Native);
    }

    #[test]
    fn win_ime_mode_for_language() {
        assert_eq!(WinImeMode::for_language("zh"), WinImeMode::Native);
        assert_eq!(WinImeMode::for_language("ja"), WinImeMode::Native);
        assert_eq!(WinImeMode::for_language("ko"), WinImeMode::Native);
        assert_eq!(WinImeMode::for_language("en"), WinImeMode::Alphanumeric);
        assert_eq!(WinImeMode::for_language("fr"), WinImeMode::LayoutOnly);
    }

    #[test]
    fn source_info_has_name_and_language() {
        let s = SourceInfo {
            platform: "windows".to_string(),
            id: "00000411".to_string(),
            name: "Japanese".to_string(),
            raw_language: "0411".to_string(),
            language: "ja".to_string(),
            is_selectable: true,
        };
        assert_eq!(s.language, "ja");
        assert_eq!(s.name, "Japanese");
    }
}
