//! Windows virtual-key ↔ logical `Key` mapping.
//!
//! We work at the physical VK level before the active IME can convert text.

use imeswitch_core::Key;

pub const VK_SEMICOLON: u32 = 0xBA; // VK_OEM_1 on US keyboards.

/// Map a leader character (e.g. `;`, `,`, `/`) to the Windows virtual-key code
/// that produces it without modifiers on a US-QWERTY layout. Returns `None`
/// for characters that need Shift on US-QWERTY (Shift collides with the
/// modifier guard).
pub fn leader_vk_for(c: char) -> Option<u32> {
    match c {
        ';' => Some(0xBA),
        ',' => Some(0xBC),
        '.' => Some(0xBE),
        '/' => Some(0xBF),
        '\'' => Some(0xDE),
        '\\' => Some(0xDC),
        '[' => Some(0xDB),
        ']' => Some(0xDD),
        '-' => Some(0xBD),
        '=' => Some(0xBB),
        '`' => Some(0xC0),
        c if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u32),
        _ => None,
    }
}

pub fn vk_to_key(vk: u32) -> Key {
    vk_to_key_with_leader(vk, VK_SEMICOLON)
}

pub fn vk_to_key_with_leader(vk: u32, leader_vk: u32) -> Key {
    if vk == leader_vk {
        return Key::Leader;
    }
    match vk {
        0x30..=0x39 => Key::alpha_num(char::from_u32(vk).unwrap_or('\0')),
        0x41..=0x5A => Key::alpha_num(char::from_u32(vk).unwrap_or('\0')),
        _ => Key::Other,
    }
}

pub fn key_to_vk(key: Key) -> Option<u32> {
    key_to_vk_with_leader(key, VK_SEMICOLON)
}

pub fn key_to_vk_with_leader(key: Key, leader_vk: u32) -> Option<u32> {
    match key {
        Key::Leader => Some(leader_vk),
        Key::AlphaNum(c) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u32),
        Key::Other => None,
        Key::AlphaNum(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_keys_round_trip() {
        for key in [
            Key::Leader,
            Key::alpha_num('e'),
            Key::alpha_num('n'),
            Key::alpha_num('j'),
            Key::alpha_num('a'),
            Key::alpha_num('z'),
            Key::alpha_num('h'),
            Key::alpha_num('7'),
        ] {
            let vk = key_to_vk(key).unwrap();
            assert_eq!(vk_to_key(vk), key);
        }
    }

    #[test]
    fn custom_leader_remaps_vk() {
        let comma_vk = leader_vk_for(',').unwrap();
        assert_eq!(vk_to_key_with_leader(comma_vk, comma_vk), Key::Leader);
        assert_eq!(
            vk_to_key_with_leader(VK_SEMICOLON, comma_vk),
            Key::Other
        );
        assert_eq!(
            key_to_vk_with_leader(Key::Leader, comma_vk),
            Some(comma_vk)
        );
    }
}
