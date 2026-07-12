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

/// Installs a HID-level keydown tap on a dedicated thread and CFRunLoop and
/// drives the trigger state machine for every keydown.
///
/// Lifetime: keep the EventHook instance alive for as long as the hook is
/// installed. `deinit` removes the run-loop source and invalidates the port.
///
/// Threading: the state machine and callback are confined to the hook thread.
/// UI work is dispatched through the closures supplied by HookService, while
/// synthetic replay is serialized on a separate queue after the callback exits.
final class EventHook {
    private static let syntheticReplayMarker: Int64 = 0x534c_4950_4b45_5901

    private var stateMachine: StateMachine
    private let leaderKeycode: UInt16
    private let onSwitch: (String) -> Void
    private let onLog: (String) -> Void

    private let replayQueue = DispatchQueue(label: "dev.zlb.imeswitch.replay", qos: .userInteractive)
    private let lifecycleLock = NSLock()
    private let started = DispatchSemaphore(value: 0)
    private let stopped = DispatchSemaphore(value: 0)

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var hookRunLoop: CFRunLoop?
    private var hookThread: Thread?
    private var startupError: Error?

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
        let thread = Thread { [weak self] in
            self?.runHookThread()
        }
        thread.name = "Slipkey EventTap"

        lifecycleLock.lock()
        hookThread = thread
        lifecycleLock.unlock()
        thread.start()
        started.wait()

        lifecycleLock.lock()
        let error = startupError
        let installed = tap != nil
        lifecycleLock.unlock()

        if let error { throw error }
        if !installed { throw EventHookError.tapCreationFailed }
    }

    private func runHookThread() {
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
            reportStartupFailure(.tapCreationFailed)
            return
        }
        guard let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0) else {
            CFMachPortInvalidate(tap)
            reportStartupFailure(.runLoopSourceFailed)
            return
        }

        let runLoop = CFRunLoopGetCurrent()
        lifecycleLock.lock()
        self.tap = tap
        runLoopSource = src
        hookRunLoop = runLoop
        lifecycleLock.unlock()

        CFRunLoopAddSource(runLoop, src, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        started.signal()
        CFRunLoopRun()

        CGEvent.tapEnable(tap: tap, enable: false)
        CFRunLoopRemoveSource(runLoop, src, .commonModes)
        CFMachPortInvalidate(tap)

        lifecycleLock.lock()
        self.tap = nil
        runLoopSource = nil
        hookRunLoop = nil
        hookThread = nil
        lifecycleLock.unlock()
        stopped.signal()
    }

    private func reportStartupFailure(_ error: EventHookError) {
        lifecycleLock.lock()
        startupError = error
        hookThread = nil
        lifecycleLock.unlock()
        started.signal()
    }

    func uninstall() {
        lifecycleLock.lock()
        let runLoop = hookRunLoop
        lifecycleLock.unlock()
        guard let runLoop else { return }

        CFRunLoopPerformBlock(runLoop, CFRunLoopMode.commonModes.rawValue) {
            CFRunLoopStop(runLoop)
        }
        CFRunLoopWakeUp(runLoop)
        stopped.wait()
    }

    var isEnabled: Bool {
        lifecycleLock.lock()
        defer { lifecycleLock.unlock() }
        return tap.map { CGEvent.tapIsEnabled(tap: $0) } ?? false
    }

    deinit {
        uninstall()
    }

    // MARK: - C-style callback

    private static let tapCallback: CGEventTapCallBack = { _, type, event, info in
        guard let info = info else { return Unmanaged.passUnretained(event) }
        let hook = Unmanaged<EventHook>.fromOpaque(info).takeUnretainedValue()

        // Re-enable the tap if the system disabled it (timeout / user input switch).
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            hook.reenableTap()
            return Unmanaged.passUnretained(event)
        }

        if EventHook.isSyntheticReplayEvent(event) {
            return Unmanaged.passUnretained(event)
        }

        guard type == .keyDown else { return Unmanaged.passUnretained(event) }
        return hook.handleKeyDown(event)
    }

    private func reenableTap() {
        lifecycleLock.lock()
        let tap = self.tap
        lifecycleLock.unlock()
        if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
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
            enqueueSwitch(lang)
        }

        let replay = Self.replayKeys(
            for: response,
            currentKey: key,
            currentKeycode: keycode,
            leaderKeycode: leaderKeycode
        )
        if !replay.isEmpty {
            enqueueReplay(replay)
        }

        if response.suppress || Self.shouldSuppressCurrentEventForAsyncReplay(response: response, currentKey: key) {
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

    static func replayKeys(
        for response: StateMachineResponse,
        currentKey: HookKey,
        currentKeycode: UInt16,
        leaderKeycode: UInt16
    ) -> [UInt16] {
        var keys = response.replay.compactMap { Keycode.fromKey($0, leader: leaderKeycode) }
        if shouldSuppressCurrentEventForAsyncReplay(response: response, currentKey: currentKey) {
            keys.append(currentKeycode)
        }
        return keys
    }

    static func shouldSuppressCurrentEventForAsyncReplay(response: StateMachineResponse, currentKey: HookKey) -> Bool {
        !response.suppress && !response.replay.isEmpty && currentKey != .other
    }

    private static func eventKey(keycode: UInt16, flags: CGEventFlags, leader: UInt16) -> HookKey {
        let blocking: CGEventFlags = [.maskShift, .maskControl, .maskAlternate, .maskCommand]
        if !flags.intersection(blocking).isEmpty {
            return .other
        }
        return Keycode.toKey(keycode, leader: leader)
    }

    private func enqueueSwitch(_ language: String) {
        DispatchQueue.main.async { [weak self] in
            self?.onSwitch(language)
        }
    }

    private func enqueueReplay(_ keycodes: [UInt16]) {
        guard !keycodes.isEmpty else { return }
        replayQueue.async {
            for keycode in keycodes {
                Self.synthPost(keycode: keycode)
            }
        }
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
