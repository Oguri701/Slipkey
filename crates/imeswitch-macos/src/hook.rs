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
//! - **Composition heuristic**: if the active input source is an IME AND
//!   the user typed something recently, assume a composition buffer is
//!   pending and let the IME handle the event. Rough but cheap; avoids
//!   eating `;` mid-Chinese/Japanese compose.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};

use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use imeswitch_core::{Language, StateMachine};

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
    on_switch: Box<dyn FnMut(Language) + Send + 'static>,
    last_keydown: Option<Instant>,
}

pub struct EventHook {
    _tap: CGEventTap<'static>,
}

impl EventHook {
    pub fn install<F>(on_switch: F) -> Result<Self, HookError>
    where
        F: FnMut(Language) + Send + 'static,
    {
        let state: &'static Mutex<HookState> = Box::leak(Box::new(Mutex::new(HookState {
            sm: StateMachine::new(),
            on_switch: Box::new(on_switch),
            last_keydown: None,
        })));

        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![CGEventType::KeyDown],
            move |_proxy, event_type, event| {
                if !matches!(event_type, CGEventType::KeyDown) {
                    return CallbackResult::Keep;
                }

                let keycode = event
                    .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                    as u16;
                let flags = event.get_flags();
                let now = Instant::now();

                // Defer to IME if we're inside a likely composition and not
                // already mid-sequence. Mid-sequence wins because the user
                // already committed to a trigger path (the leader was grabbed)
                // and dropping it now would eat keystrokes unnoticed.
                {
                    let guard = state.lock().unwrap();
                    let idle_sm = guard.sm.is_idle();
                    let recently_typed = guard
                        .last_keydown
                        .map_or(false, |t| now.duration_since(t) < COMPOSITION_IDLE_THRESHOLD);
                    drop(guard);

                    if idle_sm
                        && recently_typed
                        && current_source_kind() == CurrentSourceKind::InputMethod
                    {
                        state.lock().unwrap().last_keydown = Some(now);
                        return CallbackResult::Keep;
                    }
                }

                let key = if has_blocking_modifier(flags) {
                    imeswitch_core::Key::Other
                } else {
                    keycode_to_key(keycode)
                };

                let response = {
                    let mut guard = state.lock().unwrap();
                    let resp = guard.sm.on_keydown(key);
                    if let Some(lang) = resp.switch {
                        (guard.on_switch)(lang);
                    }
                    guard.last_keydown = Some(now);
                    resp
                };

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
}
