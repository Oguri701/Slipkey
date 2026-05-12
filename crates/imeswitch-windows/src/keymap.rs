//! Windows virtual-key ↔ logical `Key` mapping.
//!
//! We work at the physical VK level before the active IME can convert text.

use imeswitch_core::Key;

pub const SC_SEMICOLON: u32 = 0x27; // Physical semicolon key position.

/// Map a leader character (e.g. `;`, `,`, `/`) to the US-QWERTY physical
/// scan code for that key position. Windows VKs are layout-dependent for OEM
/// punctuation keys, so leader detection must use scan codes instead.
pub fn leader_scan_code_for(c: char) -> Option<u32> {
    match c {
        '`' => Some(0x29),
        '1' => Some(0x02),
        '2' => Some(0x03),
        '3' => Some(0x04),
        '4' => Some(0x05),
        '5' => Some(0x06),
        '6' => Some(0x07),
        '7' => Some(0x08),
        '8' => Some(0x09),
        '9' => Some(0x0A),
        '0' => Some(0x0B),
        '-' => Some(0x0C),
        '=' => Some(0x0D),
        'q' | 'Q' => Some(0x10),
        'w' | 'W' => Some(0x11),
        'e' | 'E' => Some(0x12),
        'r' | 'R' => Some(0x13),
        't' | 'T' => Some(0x14),
        'y' | 'Y' => Some(0x15),
        'u' | 'U' => Some(0x16),
        'i' | 'I' => Some(0x17),
        'o' | 'O' => Some(0x18),
        'p' | 'P' => Some(0x19),
        '[' => Some(0x1A),
        ']' => Some(0x1B),
        'a' | 'A' => Some(0x1E),
        's' | 'S' => Some(0x1F),
        'd' | 'D' => Some(0x20),
        'f' | 'F' => Some(0x21),
        'g' | 'G' => Some(0x22),
        'h' | 'H' => Some(0x23),
        'j' | 'J' => Some(0x24),
        'k' | 'K' => Some(0x25),
        'l' | 'L' => Some(0x26),
        ';' => Some(SC_SEMICOLON),
        '\'' => Some(0x28),
        '\\' => Some(0x2B),
        'z' | 'Z' => Some(0x2C),
        'x' | 'X' => Some(0x2D),
        'c' | 'C' => Some(0x2E),
        'v' | 'V' => Some(0x2F),
        'b' | 'B' => Some(0x30),
        'n' | 'N' => Some(0x31),
        'm' | 'M' => Some(0x32),
        ',' => Some(0x33),
        '.' => Some(0x34),
        '/' => Some(0x35),
        _ => None,
    }
}

pub fn is_leader_key_event(scan_code: u32, leader_scan_code: u32) -> bool {
    scan_code == leader_scan_code
}

pub fn vk_to_key_event_with_leader(vk: u32, scan_code: u32, leader_scan_code: u32) -> Key {
    if is_leader_key_event(scan_code, leader_scan_code) {
        return Key::Leader;
    }
    match vk {
        0x30..=0x39 => Key::alpha_num(char::from_u32(vk).unwrap_or('\0')),
        0x41..=0x5A => Key::alpha_num(char::from_u32(vk).unwrap_or('\0')),
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_leader_is_physical_semicolon_scan_code() {
        assert_eq!(leader_scan_code_for(';'), Some(SC_SEMICOLON));
    }

    #[test]
    fn custom_leader_remaps_physical_scan_code() {
        let comma_scan = leader_scan_code_for(',').unwrap();
        assert_eq!(
            vk_to_key_event_with_leader(0xBC, comma_scan, comma_scan),
            Key::Leader
        );
        assert_eq!(
            vk_to_key_event_with_leader(0xBA, SC_SEMICOLON, comma_scan),
            Key::Other
        );
    }

    #[test]
    fn default_leader_accepts_japanese_keyboard_semicolon_only_at_semicolon_scan_code() {
        assert_eq!(
            vk_to_key_event_with_leader(0xBB, SC_SEMICOLON, SC_SEMICOLON),
            Key::Leader
        );
        assert_eq!(
            vk_to_key_event_with_leader(0xBA, SC_SEMICOLON, SC_SEMICOLON),
            Key::Leader
        );
        assert_eq!(
            vk_to_key_event_with_leader(0xBB, 0x0d, SC_SEMICOLON),
            Key::Other
        );
        assert_eq!(
            vk_to_key_event_with_leader(0xBA, 0x28, SC_SEMICOLON),
            Key::Other
        );
    }

    #[test]
    fn alphanumeric_keys_still_follow_vk_for_prefixes() {
        assert_eq!(
            vk_to_key_event_with_leader(0x45, 0x12, SC_SEMICOLON),
            Key::alpha_num('E')
        );
    }
}
