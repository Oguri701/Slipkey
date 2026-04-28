import Foundation

/// Logical key recognized by the trigger state machine. Mirrors Rust `Key`.
///
/// Invariant: when constructing `.alphaNum(c)`, `c` MUST be a lowercase ASCII
/// alphanumeric character. Use `HookKey.from(character:)` to construct from
/// arbitrary input — it normalizes (lowercases ASCII, returns `.other` for
/// anything non-ASCII or non-alphanumeric).
enum HookKey: Hashable {
    case leader
    case alphaNum(Character)
    case other

    static func from(character raw: Character) -> HookKey {
        guard raw.isASCII, raw.isLetter || raw.isNumber else { return .other }
        return .alphaNum(Character(raw.lowercased()))
    }
}
