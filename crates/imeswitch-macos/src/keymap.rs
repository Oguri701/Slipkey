//! US-QWERTY physical keycode ↔ logical `Key` mapping.
//!
//! Values taken from `HIToolbox/Events.h` (`kVK_ANSI_*`). We work at the
//! physical-key level so the IME's current locale doesn't affect recognition.

use imeswitch_core::Key;

pub const KC_SEMICOLON: u16 = 0x29;
pub const KC_A: u16 = 0x00;
pub const KC_E: u16 = 0x0E;
pub const KC_J: u16 = 0x26;
pub const KC_Z: u16 = 0x06;
pub const KC_N: u16 = 0x2D;
pub const KC_H: u16 = 0x04;

pub fn keycode_to_key(kc: u16) -> Key {
    match kc {
        KC_SEMICOLON => Key::Leader,
        KC_A => Key::A,
        KC_E => Key::E,
        KC_J => Key::J,
        KC_Z => Key::Z,
        KC_N => Key::N,
        KC_H => Key::H,
        _ => Key::Other,
    }
}

pub fn key_to_keycode(k: Key) -> Option<u16> {
    match k {
        Key::Leader => Some(KC_SEMICOLON),
        Key::A => Some(KC_A),
        Key::E => Some(KC_E),
        Key::J => Some(KC_J),
        Key::Z => Some(KC_Z),
        Key::N => Some(KC_N),
        Key::H => Some(KC_H),
        Key::Other => None,
    }
}
