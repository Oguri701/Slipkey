//! US-QWERTY physical keycode ↔ logical `Key` mapping.
//!
//! Values taken from `HIToolbox/Events.h` (`kVK_ANSI_*`). We work at the
//! physical-key level so the IME's current locale doesn't affect recognition.

use imeswitch_core::Key;

pub const KC_SEMICOLON: u16 = 0x29;

/// Map a leader character (e.g. `;`, `,`, `/`) to the US-QWERTY keycode that
/// produces it without modifiers. Returns `None` for characters that need
/// Shift on US-QWERTY (those would collide with the Shift modifier guard).
pub fn leader_keycode_for(c: char) -> Option<u16> {
    match c {
        ';' => Some(0x29),
        ',' => Some(0x2B),
        '.' => Some(0x2F),
        '/' => Some(0x2C),
        '\'' => Some(0x27),
        '\\' => Some(0x2A),
        '[' => Some(0x21),
        ']' => Some(0x1E),
        '-' => Some(0x1B),
        '=' => Some(0x18),
        '`' => Some(0x32),
        c if c.is_ascii_alphanumeric() => alpha_num_keycode(c.to_ascii_lowercase()),
        _ => None,
    }
}

pub fn keycode_to_key(kc: u16) -> Key {
    keycode_to_key_with_leader(kc, KC_SEMICOLON)
}

pub fn keycode_to_key_with_leader(kc: u16, leader_kc: u16) -> Key {
    if kc == leader_kc {
        return Key::Leader;
    }
    match alpha_num_for_keycode(kc) {
        Some(c) => Key::alpha_num(c),
        None => Key::Other,
    }
}

pub fn key_to_keycode(k: Key) -> Option<u16> {
    key_to_keycode_with_leader(k, KC_SEMICOLON)
}

pub fn key_to_keycode_with_leader(k: Key, leader_kc: u16) -> Option<u16> {
    match k {
        Key::Leader => Some(leader_kc),
        Key::AlphaNum(c) => alpha_num_keycode(c),
        Key::Other => None,
    }
}

fn alpha_num_for_keycode(kc: u16) -> Option<char> {
    Some(match kc {
        0x00 => 'a',
        0x0B => 'b',
        0x08 => 'c',
        0x02 => 'd',
        0x0E => 'e',
        0x03 => 'f',
        0x05 => 'g',
        0x04 => 'h',
        0x22 => 'i',
        0x26 => 'j',
        0x28 => 'k',
        0x25 => 'l',
        0x2E => 'm',
        0x2D => 'n',
        0x1F => 'o',
        0x23 => 'p',
        0x0C => 'q',
        0x0F => 'r',
        0x01 => 's',
        0x11 => 't',
        0x20 => 'u',
        0x09 => 'v',
        0x0D => 'w',
        0x07 => 'x',
        0x10 => 'y',
        0x06 => 'z',
        0x1D => '0',
        0x12 => '1',
        0x13 => '2',
        0x14 => '3',
        0x15 => '4',
        0x17 => '5',
        0x16 => '6',
        0x1A => '7',
        0x1C => '8',
        0x19 => '9',
        _ => return None,
    })
}

fn alpha_num_keycode(c: char) -> Option<u16> {
    Some(match c {
        'a' => 0x00,
        'b' => 0x0B,
        'c' => 0x08,
        'd' => 0x02,
        'e' => 0x0E,
        'f' => 0x03,
        'g' => 0x05,
        'h' => 0x04,
        'i' => 0x22,
        'j' => 0x26,
        'k' => 0x28,
        'l' => 0x25,
        'm' => 0x2E,
        'n' => 0x2D,
        'o' => 0x1F,
        'p' => 0x23,
        'q' => 0x0C,
        'r' => 0x0F,
        's' => 0x01,
        't' => 0x11,
        'u' => 0x20,
        'v' => 0x09,
        'w' => 0x0D,
        'x' => 0x07,
        'y' => 0x10,
        'z' => 0x06,
        '0' => 0x1D,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '5' => 0x17,
        '6' => 0x16,
        '7' => 0x1A,
        '8' => 0x1C,
        '9' => 0x19,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_leader_is_semicolon() {
        assert_eq!(keycode_to_key(KC_SEMICOLON), Key::Leader);
        assert_eq!(key_to_keycode(Key::Leader), Some(KC_SEMICOLON));
    }

    #[test]
    fn custom_leader_remaps_keycode() {
        let comma_kc = leader_keycode_for(',').unwrap();
        assert_eq!(keycode_to_key_with_leader(comma_kc, comma_kc), Key::Leader);
        assert_eq!(
            keycode_to_key_with_leader(KC_SEMICOLON, comma_kc),
            Key::Other
        );
        assert_eq!(
            key_to_keycode_with_leader(Key::Leader, comma_kc),
            Some(comma_kc)
        );
    }

    #[test]
    fn alpha_num_round_trips() {
        for kc in [0x00u16, 0x0E, 0x26, 0x06, 0x12, 0x19] {
            let key = keycode_to_key(kc);
            assert!(matches!(key, Key::AlphaNum(_)));
            assert_eq!(key_to_keycode(key), Some(kc));
        }
    }
}
