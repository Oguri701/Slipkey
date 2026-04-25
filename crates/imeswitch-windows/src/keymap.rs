//! Windows virtual-key ↔ logical `Key` mapping.
//!
//! We work at the physical VK level before the active IME can convert text.

use imeswitch_core::Key;

pub const VK_SEMICOLON: u32 = 0xBA; // VK_OEM_1 on US keyboards.
pub const VK_A: u32 = 0x41;
pub const VK_E: u32 = 0x45;
pub const VK_H: u32 = 0x48;
pub const VK_J: u32 = 0x4A;
pub const VK_N: u32 = 0x4E;
pub const VK_Z: u32 = 0x5A;

pub fn vk_to_key(vk: u32) -> Key {
    match vk {
        VK_SEMICOLON => Key::Leader,
        VK_A => Key::A,
        VK_E => Key::E,
        VK_H => Key::H,
        VK_J => Key::J,
        VK_N => Key::N,
        VK_Z => Key::Z,
        _ => Key::Other,
    }
}

pub fn key_to_vk(key: Key) -> Option<u32> {
    match key {
        Key::Leader => Some(VK_SEMICOLON),
        Key::A => Some(VK_A),
        Key::E => Some(VK_E),
        Key::H => Some(VK_H),
        Key::J => Some(VK_J),
        Key::N => Some(VK_N),
        Key::Z => Some(VK_Z),
        Key::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_keys_round_trip() {
        for key in [Key::Leader, Key::E, Key::N, Key::J, Key::A, Key::Z, Key::H] {
            let vk = key_to_vk(key).unwrap();
            assert_eq!(vk_to_key(vk), key);
        }
    }
}
