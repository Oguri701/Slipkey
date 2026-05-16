import ApplicationServices
import Foundation

enum CompositionState {
    case active
    case inactive
    case unknown
}

enum Composition {
    /// 500ms after the last keystroke an IME is assumed to still hold a
    /// composition buffer. Matches `COMPOSITION_IDLE_THRESHOLD` in Rust.
    static let idleThreshold: TimeInterval = 0.5

    /// Pure-logic helper. The hook callback assembles its inputs and calls this.
    static func shouldDefer(
        idle: Bool,
        sourceIsInputMethod: Bool,
        state: CompositionState,
        possibleComposition: Bool,
        recentlyTyped: Bool
    ) -> Bool {
        guard idle, sourceIsInputMethod else { return false }
        switch state {
        case .active: return true
        case .inactive: return false
        case .unknown: return possibleComposition || recentlyTyped
        }
    }

    /// The leader key starts Slipkey's trigger sequence, so uncertainty must
    /// not make it pass through as a plain semicolon. Only a confirmed marked
    /// text range should defer the leader to the active IME composition.
    static func shouldDeferLeader(sourceIsInputMethod: Bool, state: CompositionState) -> Bool {
        sourceIsInputMethod && state == .active
    }

    /// Asks Accessibility about the focused element's marked-text range.
    /// Returns `.unknown` if AX cannot answer (web views, opaque controls).
    static func focusedState() -> CompositionState {
        let system = AXUIElementCreateSystemWide()
        guard let focused = copyAttribute(system, attribute: "AXFocusedUIElement") else {
            return .unknown
        }
        let element = focused as! AXUIElement

        // Native AppKit text views report this as an AXValue/CFRange.
        if let known = markedRangeState(element, attribute: "AXMarkedTextRange") {
            return known
        }

        // Web views expose a marker-range attribute; presence alone means composing.
        if copyAttribute(element, attribute: "AXTextInputMarkedTextMarkerRange") != nil {
            return .active
        }
        return .unknown
    }

    /// Returns whether a keycode is a "composition-ending" key (Return/Space/Delete/Escape).
    /// Matches Rust's `is_composition_ending_key`.
    static func isCompositionEnding(keycode: UInt16) -> Bool {
        switch keycode {
        case 0x24, 0x31, 0x33, 0x35: return true // return, space, delete, escape
        default: return false
        }
    }

    private static func copyAttribute(_ element: AXUIElement, attribute: String) -> CFTypeRef? {
        var value: CFTypeRef?
        let err = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
        guard err == .success, let v = value else { return nil }
        return v
    }

    /// Returns `.active`/`.inactive` if the attribute is a parseable AXValue/CFRange.
    /// Returns nil for "missing" AND for "present but unparseable" — both cases let
    /// the caller fall through to the next probe (matches Rust's match-fallthrough).
    private static func markedRangeState(_ element: AXUIElement, attribute: String) -> CompositionState? {
        guard let raw = copyAttribute(element, attribute: attribute) else { return nil }
        guard CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
        let axValue = raw as! AXValue
        guard AXValueGetType(axValue) == .cfRange else { return nil }
        var range = CFRange(location: 0, length: 0)
        guard AXValueGetValue(axValue, .cfRange, &range) else { return nil }
        return range.length > 0 ? .active : .inactive
    }
}
