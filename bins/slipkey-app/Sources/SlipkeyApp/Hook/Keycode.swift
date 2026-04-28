import Foundation

enum Keycode {
    static let semicolon: UInt16 = 0x29

    /// Maps a leader character (`;`, `,`, `/`, …) to the US-QWERTY keycode
    /// that produces it without modifiers. Returns nil for chars that need
    /// Shift on US-QWERTY (those would collide with the Shift modifier guard).
    static func leaderKeycode(for char: Character) -> UInt16? {
        switch char {
        case ";": return 0x29
        case ",": return 0x2B
        case ".": return 0x2F
        case "/": return 0x2C
        case "'": return 0x27
        case "\\": return 0x2A
        case "[": return 0x21
        case "]": return 0x1E
        case "-": return 0x1B
        case "=": return 0x18
        case "`": return 0x32
        default:
            guard char.isASCII, char.isLetter || char.isNumber else { return nil }
            return alphaNumKeycode(Character(char.lowercased()))
        }
    }

    static func toKey(_ kc: UInt16, leader: UInt16) -> HookKey {
        if kc == leader { return .leader }
        guard let ch = alphaNumChar(for: kc) else { return .other }
        return HookKey.from(character: ch)
    }

    static func fromKey(_ key: HookKey, leader: UInt16) -> UInt16? {
        switch key {
        case .leader: return leader
        case .alphaNum(let c): return alphaNumKeycode(c)
        case .other: return nil
        }
    }

    private static func alphaNumChar(for kc: UInt16) -> Character? {
        switch kc {
        case 0x00: return "a"
        case 0x0B: return "b"
        case 0x08: return "c"
        case 0x02: return "d"
        case 0x0E: return "e"
        case 0x03: return "f"
        case 0x05: return "g"
        case 0x04: return "h"
        case 0x22: return "i"
        case 0x26: return "j"
        case 0x28: return "k"
        case 0x25: return "l"
        case 0x2E: return "m"
        case 0x2D: return "n"
        case 0x1F: return "o"
        case 0x23: return "p"
        case 0x0C: return "q"
        case 0x0F: return "r"
        case 0x01: return "s"
        case 0x11: return "t"
        case 0x20: return "u"
        case 0x09: return "v"
        case 0x0D: return "w"
        case 0x07: return "x"
        case 0x10: return "y"
        case 0x06: return "z"
        case 0x1D: return "0"
        case 0x12: return "1"
        case 0x13: return "2"
        case 0x14: return "3"
        case 0x15: return "4"
        case 0x17: return "5"
        case 0x16: return "6"
        case 0x1A: return "7"
        case 0x1C: return "8"
        case 0x19: return "9"
        default: return nil
        }
    }

    private static func alphaNumKeycode(_ c: Character) -> UInt16? {
        switch c {
        case "a": return 0x00
        case "b": return 0x0B
        case "c": return 0x08
        case "d": return 0x02
        case "e": return 0x0E
        case "f": return 0x03
        case "g": return 0x05
        case "h": return 0x04
        case "i": return 0x22
        case "j": return 0x26
        case "k": return 0x28
        case "l": return 0x25
        case "m": return 0x2E
        case "n": return 0x2D
        case "o": return 0x1F
        case "p": return 0x23
        case "q": return 0x0C
        case "r": return 0x0F
        case "s": return 0x01
        case "t": return 0x11
        case "u": return 0x20
        case "v": return 0x09
        case "w": return 0x0D
        case "x": return 0x07
        case "y": return 0x10
        case "z": return 0x06
        case "0": return 0x1D
        case "1": return 0x12
        case "2": return 0x13
        case "3": return 0x14
        case "4": return 0x15
        case "5": return 0x17
        case "6": return 0x16
        case "7": return 0x1A
        case "8": return 0x1C
        case "9": return 0x19
        default: return nil
        }
    }
}
