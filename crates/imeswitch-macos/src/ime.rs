//! Wraps the Carbon framework's Text Input Services (TIS) API to let us pick
//! an input source by bundle id. TIS is still the only stable macOS surface for
//! programmatic IME selection, despite Carbon being deprecated overall.
//!
//! The default mapping (Language → input source id) covers Apple's built-ins.
//! Users on third-party IMEs (Rime, Sogou, ATOK) should swap the ids in a
//! future `ImeSwitcher::with_mapping()` call — M2 concern.

use std::collections::HashMap;
use std::os::raw::c_void;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

use imeswitch_core::Language;

/// Opaque `TISInputSourceRef` from Carbon.
#[allow(non_camel_case_types)]
type TISInputSourceRef = *const c_void;
#[allow(non_camel_case_types)]
type OSStatus = i32;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCreateInputSourceList(
        properties: CFTypeRef, // CFDictionaryRef, null for "all"
        include_all_installed: bool,
    ) -> CFArrayRef;

    fn TISSelectInputSource(source: TISInputSourceRef) -> OSStatus;

    fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;

    fn TISGetInputSourceProperty(
        source: TISInputSourceRef,
        property_key: CFStringRef,
    ) -> *mut c_void;

    static kTISPropertyInputSourceID: CFStringRef;
    static kTISPropertyLocalizedName: CFStringRef;
    static kTISPropertyInputSourceCategory: CFStringRef;
    static kTISPropertyInputSourceType: CFStringRef;
    static kTISPropertyInputSourceIsEnabled: CFStringRef;
    static kTISPropertyInputSourceIsSelectCapable: CFStringRef;
    static kTISPropertyInputSourceLanguages: CFStringRef;
}

#[derive(Debug)]
pub enum SwitchError {
    NotInstalled(String),
    NotSelectable(String),
    SelectFailed { id: String, status: OSStatus },
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchError::NotInstalled(id) => write!(
                f,
                "input source '{}' not installed — enable it in System Settings → Keyboard → Input Sources",
                id
            ),
            SwitchError::NotSelectable(id) => write!(
                f,
                "input source '{}' is installed but not selectable — use `imeswitchd list` and choose a selectable input-mode ID",
                id
            ),
            SwitchError::SelectFailed { id, status } => {
                write!(f, "TISSelectInputSource('{}') failed with OSStatus {}", id, status)
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
                source: "com.apple.keylayout.ABC".to_string(),
            },
            MappingEntry {
                language: Language::from("ja"),
                prefix: "ja".to_string(),
                source: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese".to_string(),
            },
            MappingEntry {
                language: Language::from("zh"),
                prefix: "zh".to_string(),
                source: "com.apple.inputmethod.SCIM.Shuangpin".to_string(),
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
        self.select_by_id(id)
    }

    fn select_by_id(&self, id: &str) -> Result<(), SwitchError> {
        let id_cf = CFString::new(id);
        let key_cf: CFString = unsafe { CFString::wrap_under_get_rule(kTISPropertyInputSourceID) };
        let filter = CFDictionary::from_CFType_pairs(&[(key_cf.as_CFType(), id_cf.as_CFType())]);

        let arr_ref = unsafe { TISCreateInputSourceList(filter.as_CFTypeRef(), false) };
        if arr_ref.is_null() {
            return Err(SwitchError::NotInstalled(id.to_string()));
        }
        let array: CFArray<CFType> = unsafe { CFArray::wrap_under_create_rule(arr_ref) };
        if array.len() == 0 {
            return Err(SwitchError::NotInstalled(id.to_string()));
        }

        // First match.
        let source_item = array
            .get(0)
            .ok_or_else(|| SwitchError::NotInstalled(id.to_string()))?;
        let source_ref: TISInputSourceRef = source_item.as_CFTypeRef() as *const c_void;
        let selectable = unsafe {
            read_bool(
                source_item.as_CFTypeRef(),
                kTISPropertyInputSourceIsSelectCapable,
            )
        }
        .unwrap_or(false);
        if !selectable {
            return Err(SwitchError::NotSelectable(id.to_string()));
        }

        let status = unsafe { TISSelectInputSource(source_ref) };
        if status != 0 {
            return Err(SwitchError::SelectFailed {
                id: id.to_string(),
                status,
            });
        }
        Ok(())
    }
}

impl Default for ImeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub type_: String,
    pub languages: Vec<String>,
    pub is_enabled: bool,
    pub is_selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedIME {
    pub language: Language,
    pub source_id: String,
    pub name: String,
    pub is_selectable: bool,
}

/// Dump every input source the system reports (enabled or not).
pub fn list_all_sources() -> Vec<SourceInfo> {
    unsafe {
        let arr_ref = TISCreateInputSourceList(std::ptr::null(), true);
        if arr_ref.is_null() {
            return Vec::new();
        }
        let array: CFArray<CFType> = CFArray::wrap_under_create_rule(arr_ref);
        array
            .iter()
            .map(|item| {
                let ptr = item.as_CFTypeRef();
                SourceInfo {
                    id: read_str(ptr, kTISPropertyInputSourceID).unwrap_or_default(),
                    name: read_str(ptr, kTISPropertyLocalizedName).unwrap_or_default(),
                    category: read_str(ptr, kTISPropertyInputSourceCategory).unwrap_or_default(),
                    type_: read_str(ptr, kTISPropertyInputSourceType).unwrap_or_default(),
                    languages: read_string_array(ptr, kTISPropertyInputSourceLanguages)
                        .unwrap_or_default(),
                    is_enabled: read_bool(ptr, kTISPropertyInputSourceIsEnabled).unwrap_or(false),
                    is_selectable: read_bool(ptr, kTISPropertyInputSourceIsSelectCapable)
                        .unwrap_or(false),
                }
            })
            .collect()
    }
}

pub fn discover_installed_imes() -> Vec<DetectedIME> {
    let mut detected = Vec::new();
    for source in list_all_sources() {
        if !source.is_enabled || !source.is_selectable {
            continue;
        }
        // Keyboard layouts like ABC advertise every Latin-script tag they
        // *can* type (en, af, ca, co, da, de, ...). Only the first tag is
        // the source's primary language — surfacing the rest creates dozens
        // of phantom "AF/CA/DE" rows for IMEs the user never installed.
        let Some(language) = source
            .languages
            .iter()
            .filter_map(|tag| iso_code_from_bcp47(tag))
            .next()
        else {
            continue;
        };
        detected.push(DetectedIME {
            language,
            source_id: source.id,
            name: source.name,
            is_selectable: source.is_selectable,
        });
    }
    detected.sort_by(|a, b| {
        a.language
            .cmp(&b.language)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.source_id.cmp(&b.source_id))
    });
    detected.dedup_by(|a, b| a.language == b.language && a.source_id == b.source_id);
    detected
}

/// Ask the system for the CURRENTLY active keyboard input source ID.
/// Logs whatever TIS considers "active", which may differ from the menu-bar
/// display in degenerate cases — that's precisely the symptom we're debugging.
pub fn current_source_id() -> Option<String> {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        if src.is_null() {
            return None;
        }
        let owned: CFType = CFType::wrap_under_create_rule(src as CFTypeRef);
        read_str(owned.as_CFTypeRef(), kTISPropertyInputSourceID)
    }
}

/// High-level classification of the active input source. We treat anything
/// that isn't a pure keyboard layout as potentially composing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentSourceKind {
    /// A pure keyboard layout (ABC, Dvorak, Russian, …). No IME composition
    /// is possible — safe to always grab triggers.
    KeyboardLayout,
    /// An input method or input mode (SCIM.Shuangpin, Kotoeri.Japanese, …).
    /// Composition buffer may exist; see the composition-aware heuristic in
    /// hook.rs before deciding whether to grab a trigger.
    InputMethod,
    /// Palette, dictation, etc. — neither layout nor IME in the usual sense.
    Other,
}

pub fn current_source_kind() -> CurrentSourceKind {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        if src.is_null() {
            return CurrentSourceKind::Other;
        }
        let owned: CFType = CFType::wrap_under_create_rule(src as CFTypeRef);
        match read_str(owned.as_CFTypeRef(), kTISPropertyInputSourceType).as_deref() {
            Some("TISTypeKeyboardLayout") => CurrentSourceKind::KeyboardLayout,
            Some("TISTypeKeyboardInputMode")
            | Some("TISTypeKeyboardInputMethodWithoutModes")
            | Some("TISTypeKeyboardInputMethodModeEnabled") => CurrentSourceKind::InputMethod,
            _ => CurrentSourceKind::Other,
        }
    }
}

unsafe fn read_str(src: CFTypeRef, key: CFStringRef) -> Option<String> {
    let val_ptr = TISGetInputSourceProperty(src as *const c_void, key);
    if val_ptr.is_null() {
        return None;
    }
    let cfstr = CFString::wrap_under_get_rule(val_ptr as CFStringRef);
    Some(cfstr.to_string())
}

unsafe fn read_bool(src: CFTypeRef, key: CFStringRef) -> Option<bool> {
    let val_ptr = TISGetInputSourceProperty(src as *const c_void, key);
    if val_ptr.is_null() {
        return None;
    }
    // CFBoolean: equal to kCFBooleanTrue iff true. Compare via CFBooleanGetValue.
    extern "C" {
        fn CFBooleanGetValue(boolean: *const c_void) -> u8;
    }
    Some(CFBooleanGetValue(val_ptr) != 0)
}

unsafe fn read_string_array(src: CFTypeRef, key: CFStringRef) -> Option<Vec<String>> {
    let val_ptr = TISGetInputSourceProperty(src as *const c_void, key);
    if val_ptr.is_null() {
        return None;
    }
    let array: CFArray<CFString> = CFArray::wrap_under_get_rule(val_ptr as CFArrayRef);
    Some(array.iter().map(|value| value.to_string()).collect())
}

fn iso_code_from_bcp47(tag: &str) -> Option<Language> {
    let code = tag
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(Language::from(code))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcp47_tags_map_to_iso_codes() {
        assert_eq!(iso_code_from_bcp47("zh-Hans"), Some(Language::from("zh")));
        assert_eq!(iso_code_from_bcp47("ja_JP"), Some(Language::from("ja")));
        assert_eq!(iso_code_from_bcp47("fr"), Some(Language::from("fr")));
        assert_eq!(iso_code_from_bcp47("root"), None);
    }
}
