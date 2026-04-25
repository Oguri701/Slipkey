//! A `CGEventTap` at HID level that feeds every keydown through the
//! trigger state machine. On trigger completion we invoke the user's
//! `on_switch` callback; on cancellation we synth-post the buffered
//! keys at the Session tap so they reach the IME/app as normal input
//! but don't loop back into our own HID tap.
//!
//! Two guards sit in front of the state machine:
//!
//! - **Modifier mask**: Shift/Ctrl/Option/Cmd held → never interpret as
//!   part of a trigger. Otherwise Shift+; (= `:`) would start a sequence.
//! - **Composition awareness**: if the active input source is an IME, ask
//!   Accessibility whether the focused text input has marked text. If AX
//!   cannot answer, fall back to a short idle-window heuristic so we still
//!   avoid eating `;` mid-Chinese/Japanese compose in opaque controls.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};

use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use imeswitch_core::{Key, Language, StateMachine};

use crate::composition::{
    focused_composition_state, should_defer_for_composition, CompositionState,
};
use crate::ime::{current_source_kind, CurrentSourceKind};
use crate::keymap::{key_to_keycode, keycode_to_key};

/// How long after the last keystroke an IME is assumed to still hold a
/// composition buffer. During this window we defer to the IME rather than
/// running trigger detection.
const COMPOSITION_IDLE_THRESHOLD: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum HookError {
    SourceCreation,
    TapCreation,
    RunLoopSource,
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HookError::SourceCreation => "CGEventSource::new failed",
            HookError::TapCreation => {
                "CGEventTap::new failed — is Accessibility permission granted?"
            }
            HookError::RunLoopSource => "create_runloop_source failed",
        };
        f.write_str(s)
    }
}

impl std::error::Error for HookError {}

struct HookState {
    sm: StateMachine,
    last_keydown: Option<Instant>,
    possible_composition: bool,
}

pub struct EventHook {
    _tap: CGEventTap<'static>,
}

impl EventHook {
    pub fn install<F>(on_switch: F) -> Result<Self, HookError>
    where
        F: FnMut(Language) + Send + 'static,
    {
        let state = Arc::new(Mutex::new(HookState {
            sm: StateMachine::new(),
            last_keydown: None,
            possible_composition: false,
        }));
        let on_switch = Arc::new(Mutex::new(
            Box::new(on_switch) as Box<dyn FnMut(Language) + Send>
        ));

        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![CGEventType::KeyDown],
            {
                let state = Arc::clone(&state);
                let on_switch = Arc::clone(&on_switch);
                move |_proxy, event_type, event| {
                    if !matches!(event_type, CGEventType::KeyDown) {
                        return CallbackResult::Keep;
                    }

                    let keycode = event
                        .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                        as u16;
                    let flags = event.get_flags();
                    let now = Instant::now();

                    let (idle_sm, last_kd, possible_composition) = {
                        let guard = state.lock().unwrap();
                        (
                            guard.sm.is_idle(),
                            guard.last_keydown,
                            guard.possible_composition,
                        )
                    };
                    let ms_since_last_kd = last_kd.map(|t| now.duration_since(t).as_millis());
                    let recently_typed = ms_since_last_kd
                        .map_or(false, |m| m < COMPOSITION_IDLE_THRESHOLD.as_millis());
                    let source_kind = current_source_kind();
                    let composition_state = if source_kind == CurrentSourceKind::InputMethod {
                        focused_composition_state()
                    } else {
                        CompositionState::Inactive
                    };
                    let should_defer = should_defer_for_composition(
                        idle_sm,
                        source_kind == CurrentSourceKind::InputMethod,
                        composition_state,
                        possible_composition,
                        recently_typed,
                    );
                    let has_mod = has_blocking_modifier(flags);

                    log::debug!(
                        "kd kc={:#04x} flags={:#010x} source={:?} composition={:?} possible={} idle={} last_ms={:?} recent={} mod={} → defer={}",
                        keycode,
                        flags.bits(),
                        source_kind,
                        composition_state,
                        possible_composition,
                        idle_sm,
                        ms_since_last_kd,
                        recently_typed,
                        has_mod,
                        should_defer,
                    );

                    if should_defer {
                        let mut guard = state.lock().unwrap();
                        guard.last_keydown = Some(now);
                        update_possible_composition(
                            &mut guard,
                            source_kind,
                            composition_state,
                            keycode,
                            false,
                        );
                        return CallbackResult::Keep;
                    }

                    let key = event_key(keycode, flags);

                    let response = {
                        let mut guard = state.lock().unwrap();
                        let resp = guard.sm.on_keydown(key);
                        guard.last_keydown = Some(now);
                        update_possible_composition(
                            &mut guard,
                            source_kind,
                            composition_state,
                            keycode,
                            resp.switch.is_some(),
                        );
                        resp
                    };

                    if let Some(lang) = response.switch {
                        let mut callback = on_switch.lock().unwrap();
                        callback(lang);
                    }

                    log::debug!(
                        "  → key={:?} suppress={} replay={:?} switch={:?}",
                        key,
                        response.suppress,
                        response.replay,
                        response.switch,
                    );

                    for k in &response.replay {
                        if let Some(kc) = key_to_keycode(*k) {
                            synth_post(kc);
                        }
                    }

                    if response.suppress {
                        CallbackResult::Drop
                    } else {
                        CallbackResult::Keep
                    }
                }
            },
        )
        .map_err(|_| HookError::TapCreation)?;

        let src = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|_| HookError::RunLoopSource)?;
        let rl = CFRunLoop::get_current();
        unsafe {
            rl.add_source(&src, kCFRunLoopCommonModes);
        }
        tap.enable();

        Ok(EventHook { _tap: tap })
    }
}

fn has_blocking_modifier(flags: CGEventFlags) -> bool {
    let mask = CGEventFlags::CGEventFlagShift
        | CGEventFlags::CGEventFlagControl
        | CGEventFlags::CGEventFlagAlternate
        | CGEventFlags::CGEventFlagCommand;
    flags.intersects(mask)
}

fn event_key(keycode: u16, flags: CGEventFlags) -> Key {
    if has_blocking_modifier(flags) {
        Key::Other
    } else {
        keycode_to_key(keycode)
    }
}

fn update_possible_composition(
    state: &mut HookState,
    source_kind: CurrentSourceKind,
    composition_state: CompositionState,
    keycode: u16,
    did_switch: bool,
) {
    if did_switch || source_kind != CurrentSourceKind::InputMethod {
        state.possible_composition = false;
        return;
    }

    match composition_state {
        CompositionState::Active => state.possible_composition = true,
        CompositionState::Inactive => state.possible_composition = false,
        CompositionState::Unknown if is_composition_ending_key(keycode) => {
            state.possible_composition = false;
        }
        CompositionState::Unknown => state.possible_composition = true,
    }
}

fn is_composition_ending_key(keycode: u16) -> bool {
    matches!(keycode, 0x24 | 0x31 | 0x33 | 0x35)
}

fn synth_post(keycode: u16) {
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    if let Ok(down) = CGEvent::new_keyboard_event(src.clone(), keycode, true) {
        down.post(CGEventTapLocation::Session);
    }
    if let Ok(up) = CGEvent::new_keyboard_event(src, keycode, false) {
        up.post(CGEventTapLocation::Session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::{should_defer_for_composition, CompositionState};
    use crate::keymap::KC_SEMICOLON;

    #[test]
    fn blocking_modifier_detects_shift_ctrl_option_cmd() {
        assert!(has_blocking_modifier(CGEventFlags::CGEventFlagShift));
        assert!(has_blocking_modifier(CGEventFlags::CGEventFlagControl));
        assert!(has_blocking_modifier(CGEventFlags::CGEventFlagAlternate));
        assert!(has_blocking_modifier(CGEventFlags::CGEventFlagCommand));
        assert!(has_blocking_modifier(
            CGEventFlags::CGEventFlagShift | CGEventFlags::CGEventFlagCommand
        ));
    }

    #[test]
    fn blocking_modifier_ignores_capslock_and_fn() {
        assert!(!has_blocking_modifier(CGEventFlags::CGEventFlagNull));
        assert!(!has_blocking_modifier(CGEventFlags::CGEventFlagAlphaShift));
        assert!(!has_blocking_modifier(CGEventFlags::CGEventFlagSecondaryFn));
    }

    #[test]
    fn shifted_leader_is_not_a_trigger_key() {
        assert_eq!(
            event_key(KC_SEMICOLON, CGEventFlags::CGEventFlagShift),
            Key::Other
        );
    }

    #[test]
    fn active_composition_defers_even_after_idle_threshold() {
        assert!(should_defer_for_composition(
            true,
            true,
            CompositionState::Active,
            false,
            false
        ));
    }

    #[test]
    fn inactive_composition_does_not_use_time_heuristic() {
        assert!(!should_defer_for_composition(
            true,
            true,
            CompositionState::Inactive,
            true,
            true
        ));
    }

    #[test]
    fn unknown_composition_falls_back_to_time_heuristic() {
        assert!(should_defer_for_composition(
            true,
            true,
            CompositionState::Unknown,
            false,
            true
        ));
        assert!(!should_defer_for_composition(
            true,
            true,
            CompositionState::Unknown,
            false,
            false
        ));
    }

    #[test]
    fn unknown_possible_composition_defers_after_idle_threshold() {
        assert!(should_defer_for_composition(
            true,
            true,
            CompositionState::Unknown,
            true,
            false
        ));
    }

    #[test]
    fn possible_composition_tracks_ime_typing_until_explicit_end() {
        let mut state = HookState {
            sm: StateMachine::new(),
            last_keydown: None,
            possible_composition: false,
        };

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x04,
            false,
        );
        assert!(state.possible_composition);

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x24,
            false,
        );
        assert!(!state.possible_composition);
    }

    #[test]
    fn space_commits_and_ends_possible_composition() {
        let mut state = HookState {
            sm: StateMachine::new(),
            last_keydown: None,
            possible_composition: false,
        };

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x04,
            false,
        );
        assert!(state.possible_composition);

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x31,
            false,
        );
        assert!(!state.possible_composition);
    }

    #[test]
    fn delete_cancels_and_ends_possible_composition() {
        let mut state = HookState {
            sm: StateMachine::new(),
            last_keydown: None,
            possible_composition: false,
        };

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x04,
            false,
        );
        assert!(state.possible_composition);

        update_possible_composition(
            &mut state,
            CurrentSourceKind::InputMethod,
            CompositionState::Unknown,
            0x33,
            false,
        );
        assert!(!state.possible_composition);
    }
}
