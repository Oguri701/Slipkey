//! A `CGEventTap` at HID level that feeds every keydown through the
//! trigger state machine. On trigger completion we invoke the user's
//! `on_switch` callback; on cancellation we synth-post the buffered
//! keys at the Session tap so they reach the IME/app as normal input
//! but don't loop back into our own HID tap.

use std::sync::Mutex;

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};

use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use imeswitch_core::{Language, StateMachine};

use crate::keymap::{key_to_keycode, keycode_to_key};

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
}

pub struct EventHook {
    // Keep the tap alive for the lifetime of the hook. We intentionally don't
    // expose disable/reinstall for M0 — the process exit takes it down.
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
                let key = keycode_to_key(keycode);

                let response = {
                    let mut guard = state.lock().unwrap();
                    let resp = guard.sm.on_keydown(key);
                    if let Some(lang) = resp.switch {
                        (guard.on_switch)(lang);
                    }
                    resp
                };

                // Synth-post buffered replay keys at Session level so they
                // reach IME/app but don't re-enter our HID tap.
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
