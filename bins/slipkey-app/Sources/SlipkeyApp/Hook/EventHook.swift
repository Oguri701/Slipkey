import AppKit
import CoreGraphics
import Foundation

enum EventHookError: Error, CustomStringConvertible {
    case tapCreationFailed
    case runLoopSourceFailed

    var description: String {
        switch self {
        case .tapCreationFailed:
            return "CGEvent.tapCreate returned nil — Accessibility permission not granted to Slipkey"
        case .runLoopSourceFailed:
            return "CFMachPortCreateRunLoopSource returned nil"
        }
    }
}

/// Installs a HID-level keydown tap on the current thread's CFRunLoop and
/// drives the trigger state machine for every keydown.
///
/// Lifetime: keep the EventHook instance alive for as long as the hook is
/// installed. `deinit` removes the run-loop source and invalidates the port.
///
/// Threading: every call site (install, uninstall, the C callback) runs on the
/// main thread because the tap's run-loop source is added to the main run loop.
/// We don't mark this class `@MainActor` because the C callback is a static
/// `@convention(c)` function with no ambient actor; isolating the class would
/// force every method call through awkward `MainActor.assumeIsolated` blocks.
final class EventHook {
    private static let syntheticReplayMarker: Int64 = 0x534c_4950_4b45_5901

    private var stateMachine: StateMachine
    private let leaderKeycode: UInt16
    private let onSwitch: (String) -> Void
    private let onLog: (String) -> Void

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?

    init(
        leaderKeycode: UInt16,
        mappings: [(language: String, prefix: String)],
        onSwitch: @escaping (String) -> Void,
        onLog: @escaping (String) -> Void = { _ in }
    ) {
        self.stateMachine = StateMachine(mappings: mappings)
        self.leaderKeycode = leaderKeycode
        self.onSwitch = onSwitch
        self.onLog = onLog
    }

    func install() throws {
        let mask: CGEventMask = 1 << CGEventType.keyDown.rawValue
        let info = Unmanaged.passUnretained(self).toOpaque()

        guard let tap = CGEvent.tapCreate(
            tap: .cghidEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: Self.tapCallback,
            userInfo: info
        ) else {
            throw EventHookError.tapCreationFailed
        }
        guard let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0) else {
            CFMachPortInvalidate(tap)
            throw EventHookError.runLoopSourceFailed
        }
        CFRunLoopAddSource(CFRunLoopGetCurrent(), src, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        self.tap = tap
        self.runLoopSource = src
    }

    func uninstall() {
        if let src = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), src, .commonModes)
            self.runLoopSource = nil
        }
        if let tap = tap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
            self.tap = nil
        }
    }

    var isEnabled: Bool {
        guard let tap else { return false }
        return CGEvent.tapIsEnabled(tap: tap)
    }

    deinit {
        if let src = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), src, .commonModes)
        }
        if let tap = tap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
        }
    }

    // MARK: - C-style callback

    private static let tapCallback: CGEventTapCallBack = { _, type, event, info in
        guard let info = info else { return Unmanaged.passUnretained(event) }
        let hook = Unmanaged<EventHook>.fromOpaque(info).takeUnretainedValue()

        // Re-enable the tap if the system disabled it (timeout / user input switch).
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let tap = hook.tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return Unmanaged.passUnretained(event)
        }

        if EventHook.isSyntheticReplayEvent(event) {
            return Unmanaged.passUnretained(event)
        }

        guard type == .keyDown else { return Unmanaged.passUnretained(event) }
        return hook.handleKeyDown(event)
    }

    private func handleKeyDown(_ event: CGEvent) -> Unmanaged<CGEvent>? {
        let keycode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        let flags = event.flags
        let key = Self.eventKey(keycode: keycode, flags: flags, leader: leaderKeycode)

        let idle = stateMachine.isIdle

        if Self.shouldInspectComposition(idle: idle, key: key) {
            let sourceKind = IMEManager.currentSourceKind()
            let compositionState: CompositionState
            if sourceKind == .inputMethod {
                compositionState = Composition.focusedState()
            } else {
                compositionState = .inactive
            }
            let shouldDefer = Composition.shouldDeferLeader(
                sourceIsInputMethod: sourceKind == .inputMethod,
                state: compositionState
            )

            if shouldDefer {
                return Unmanaged.passUnretained(event)
            }
        }

        let response = stateMachine.onKeydown(key)

        if let lang = response.switchTo {
            onSwitch(lang)
        }

        for k in response.replay {
            if let kc = Keycode.fromKey(k, leader: leaderKeycode) {
                Self.synthPost(keycode: kc)
            }
        }

        if response.suppress {
            return nil  // drop the event
        }
        return Unmanaged.passUnretained(event)
    }

    static func shouldInspectComposition(idle: Bool, key: HookKey) -> Bool {
        idle && key == .leader
    }

    static func isSyntheticReplayEvent(_ event: CGEvent) -> Bool {
        event.getIntegerValueField(.eventSourceUserData) == syntheticReplayMarker
    }

    private static func eventKey(keycode: UInt16, flags: CGEventFlags, leader: UInt16) -> HookKey {
        let blocking: CGEventFlags = [.maskShift, .maskControl, .maskAlternate, .maskCommand]
        if !flags.intersection(blocking).isEmpty {
            return .other
        }
        return Keycode.toKey(keycode, leader: leader)
    }

    /// Synth-post the given keycode at session level (not HID) so it doesn't
    /// re-enter our own tap.
    private static func synthPost(keycode: UInt16) {
        guard let src = CGEventSource(stateID: .hidSystemState) else { return }
        if let down = CGEvent(keyboardEventSource: src, virtualKey: keycode, keyDown: true) {
            markSyntheticReplay(down)
            down.post(tap: .cgSessionEventTap)
        }
        if let up = CGEvent(keyboardEventSource: src, virtualKey: keycode, keyDown: false) {
            markSyntheticReplay(up)
            up.post(tap: .cgSessionEventTap)
        }
    }

    private static func markSyntheticReplay(_ event: CGEvent) {
        event.setIntegerValueField(.eventSourceUserData, value: syntheticReplayMarker)
    }
}
