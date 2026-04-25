//! Best-effort macOS composition detection.
//!
//! AppKit does not expose a cheap, universal "is the focused text input
//! composing?" API at the event-tap layer. We ask Accessibility first for
//! focused-element marked-text attributes. Some controls expose these directly;
//! web views often expose a text-marker range instead. When AX cannot answer,
//! the hook falls back to its older short idle-window heuristic.

use std::os::raw::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use core_foundation::base::{CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowAlpha, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowName,
    kCGWindowOwnerName,
};

#[allow(non_camel_case_types)]
type AXUIElementRef = *const c_void;
#[allow(non_camel_case_types)]
type AXError = i32;
#[allow(non_camel_case_types)]
type AXValueRef = *const c_void;
#[allow(non_camel_case_types)]
type AXValueType = u32;

const K_AX_ERROR_SUCCESS: AXError = 0;
const K_AX_VALUE_CF_RANGE_TYPE: AXValueType = 4;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXValueGetTypeID() -> CFTypeID;
    fn AXValueGetType(value: AXValueRef) -> AXValueType;
    fn AXValueGetValue(value: AXValueRef, the_type: AXValueType, value_ptr: *mut c_void) -> u8;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFDictionaryGetValueIfPresent(
        the_dict: CFDictionaryRef,
        key: *const c_void,
        value: *mut *const c_void,
    ) -> u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionState {
    Active,
    Inactive,
    Unknown,
}

pub fn focused_composition_state() -> CompositionState {
    // AX answers authoritatively for native AppKit text views — fast path.
    // Only fall back to the (much heavier) candidate-window scan when AX is
    // ambiguous, e.g. opaque web views or non-AppKit controls.
    match focused_ax_composition_state() {
        s @ (CompositionState::Active | CompositionState::Inactive) => s,
        CompositionState::Unknown => {
            if ime_candidate_window_is_visible_cached() {
                CompositionState::Active
            } else {
                CompositionState::Unknown
            }
        }
    }
}

/// Window enumeration via `copy_window_info` is expensive (Window-Server IPC,
/// list every on-screen window). Cap that to ~10 calls/sec by caching the
/// last result; typing rhythm is well below that anyway.
const CANDIDATE_WINDOW_CACHE_TTL: Duration = Duration::from_millis(100);
static CANDIDATE_WINDOW_CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

fn ime_candidate_window_is_visible_cached() -> bool {
    let now = Instant::now();
    if let Ok(guard) = CANDIDATE_WINDOW_CACHE.lock() {
        if let Some((stamp, value)) = *guard {
            if now.duration_since(stamp) < CANDIDATE_WINDOW_CACHE_TTL {
                return value;
            }
        }
    }
    let value = ime_candidate_window_is_visible();
    if let Ok(mut guard) = CANDIDATE_WINDOW_CACHE.lock() {
        *guard = Some((now, value));
    }
    value
}

fn focused_ax_composition_state() -> CompositionState {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return CompositionState::Unknown;
        }
        let _system_owner = CFType::wrap_under_create_rule(system as CFTypeRef);

        let Some(focused) = copy_attribute(system, "AXFocusedUIElement") else {
            return CompositionState::Unknown;
        };
        let focused_ref = focused.as_CFTypeRef() as AXUIElementRef;
        focused_element_composition_state(focused_ref)
    }
}

pub(crate) fn ime_candidate_window_is_visible() -> bool {
    let Some(windows) = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return false;
    };

    for window in windows.iter() {
        let dict = *window as CFDictionaryRef;
        let owner = unsafe { dictionary_string(dict, kCGWindowOwnerName) };
        if !owner_name_looks_like_input_method(&owner) {
            continue;
        }

        let name = unsafe { dictionary_string(dict, kCGWindowName) };
        let layer = unsafe { dictionary_i32(dict, kCGWindowLayer) }.unwrap_or(0);
        let alpha = unsafe { dictionary_f64(dict, kCGWindowAlpha) }.unwrap_or(1.0);
        if window_looks_like_candidate_ui(&owner, &name, layer, alpha) {
            return true;
        }
    }
    false
}

unsafe fn focused_element_composition_state(element: AXUIElementRef) -> CompositionState {
    match marked_cf_range_state(element, "AXMarkedTextRange") {
        CompositionState::Unknown => {}
        known => return known,
    }

    match marker_range_presence_state(element, "AXTextInputMarkedTextMarkerRange") {
        CompositionState::Unknown => CompositionState::Unknown,
        known => known,
    }
}

unsafe fn marked_cf_range_state(element: AXUIElementRef, attribute: &str) -> CompositionState {
    let Some(value) = copy_attribute(element, attribute) else {
        return CompositionState::Unknown;
    };
    let value_ref = value.as_CFTypeRef();
    // If the control returns *something* but not an AXValue/CFRange, we don't
    // know how to interpret it. Stay Unknown rather than over-defer; the
    // candidate-window cache + the time heuristic still catch real composes.
    if CFGetTypeID(value_ref) != AXValueGetTypeID() {
        return CompositionState::Unknown;
    }

    let ax_value = value_ref as AXValueRef;
    if AXValueGetType(ax_value) != K_AX_VALUE_CF_RANGE_TYPE {
        return CompositionState::Unknown;
    }

    let mut range = CFRange {
        location: 0,
        length: 0,
    };
    let ok = AXValueGetValue(
        ax_value,
        K_AX_VALUE_CF_RANGE_TYPE,
        &mut range as *mut CFRange as *mut c_void,
    );
    if ok == 0 {
        return CompositionState::Unknown;
    }
    if range.length > 0 {
        CompositionState::Active
    } else {
        CompositionState::Inactive
    }
}

unsafe fn marker_range_presence_state(
    element: AXUIElementRef,
    attribute: &str,
) -> CompositionState {
    if copy_attribute(element, attribute).is_some() {
        CompositionState::Active
    } else {
        CompositionState::Unknown
    }
}

unsafe fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Option<CFType> {
    let attr = CFString::new(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value);
    if err == K_AX_ERROR_SUCCESS && !value.is_null() {
        Some(CFType::wrap_under_create_rule(value))
    } else {
        None
    }
}

fn owner_name_looks_like_input_method(owner: &str) -> bool {
    const INPUT_METHOD_OWNERS: &[&str] = &[
        "TextInputMenuAgent",
        "TextInputSwitcher",
        "CursorUIViewService",
        "SCIM",
        "JapaneseIM",
        "Kotoeri",
        "Pinyin",
        "KeyboardIM",
        "IMK",
    ];

    INPUT_METHOD_OWNERS
        .iter()
        .any(|needle| owner.contains(needle))
}

fn window_looks_like_candidate_ui(owner: &str, name: &str, layer: i32, alpha: f64) -> bool {
    if alpha <= 0.0 || !owner_name_looks_like_input_method(owner) {
        return false;
    }

    // Candidate windows are floating UI. Regular input-method helper processes
    // can exist without composition, but they should not have layer-0 app
    // windows in the normal typing path.
    if layer > 0 {
        return true;
    }

    let lower_name = name.to_ascii_lowercase();
    lower_name.contains("candidate")
        || lower_name.contains("composition")
        || lower_name.contains("conversion")
        || lower_name.contains("候補")
        || lower_name.contains("変換")
}

unsafe fn dictionary_value(dict: CFDictionaryRef, key: CFStringRef) -> Option<CFType> {
    let mut value: *const c_void = std::ptr::null();
    let found = CFDictionaryGetValueIfPresent(dict, key as *const c_void, &mut value);
    if found != 0 && !value.is_null() {
        Some(CFType::wrap_under_get_rule(value as CFTypeRef))
    } else {
        None
    }
}

unsafe fn dictionary_string(dict: CFDictionaryRef, key: CFStringRef) -> String {
    dictionary_value(dict, key)
        .and_then(|value| value.downcast::<CFString>())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

unsafe fn dictionary_i32(dict: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    dictionary_value(dict, key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_i32())
}

unsafe fn dictionary_f64(dict: CFDictionaryRef, key: CFStringRef) -> Option<f64> {
    dictionary_value(dict, key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_f64())
}

pub(crate) fn should_defer_for_composition(
    idle: bool,
    source_is_input_method: bool,
    composition_state: CompositionState,
    possible_composition: bool,
    recently_typed: bool,
) -> bool {
    idle && source_is_input_method
        && match composition_state {
            CompositionState::Active => true,
            CompositionState::Inactive => false,
            CompositionState::Unknown => possible_composition || recently_typed,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_method_owner_names_are_recognized() {
        assert!(owner_name_looks_like_input_method("TextInputMenuAgent"));
        assert!(owner_name_looks_like_input_method("SCIM_Extension"));
        assert!(owner_name_looks_like_input_method(
            "JapaneseIM-RomajiTyping"
        ));
        assert!(!owner_name_looks_like_input_method("Codex"));
    }

    #[test]
    fn floating_input_method_window_counts_as_candidate_ui() {
        assert!(window_looks_like_candidate_ui(
            "JapaneseIM-RomajiTyping",
            "",
            25,
            1.0
        ));
        assert!(!window_looks_like_candidate_ui(
            "JapaneseIM-RomajiTyping",
            "",
            0,
            1.0
        ));
        assert!(!window_looks_like_candidate_ui(
            "Codex",
            "candidate",
            25,
            1.0
        ));
    }
}
