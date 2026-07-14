import AppKit
import Foundation

@MainActor
final class HookService {
    private var hook: EventHook?
    private var inputSourceObserver: NSObjectProtocol?

    init() {
        inputSourceObserver = NotificationCenter.default.addObserver(
            forName: NSTextInputContext.keyboardSelectionDidChangeNotification,
            object: nil,
            queue: .main
        ) { _ in
            Task { @MainActor in
                DiagnosticLogger.shared.log("input_source.notification", fields: Self.currentContextFields())
            }
        }
    }

    deinit {
        if let inputSourceObserver {
            NotificationCenter.default.removeObserver(inputSourceObserver)
        }
    }

    /// Installs the hook using the given config. Replaces any existing hook.
    /// Returns true on success, false if `EventHook.install()` failed (e.g.
    /// no Accessibility permission).
    @discardableResult
    func start(with config: SlipkeyConfig) -> Bool {
        stop()
        let leaderChar = config.leader.first ?? ";"
        let leaderKC = Keycode.leaderKeycode(for: leaderChar) ?? Keycode.semicolon
        let mappings: [(language: String, prefix: String)] = config.mappings
            .filter { $0.enabled && !$0.prefix.isEmpty }
            .map { (language: $0.language.lowercased(), prefix: $0.prefix) }

        let hook = EventHook(
            leaderKeycode: leaderKC,
            mappings: mappings,
            onSwitch: { [weak self] lang in
                self?.handleSwitch(lang: lang, config: config)
            },
            onLog: { msg in NSLog("Slipkey hook: %@", msg) }
        )
        do {
            try hook.install()
            self.hook = hook
            NSLog("Slipkey: hook installed (leader=%@, mappings=%d)", String(leaderChar), mappings.count)
            DiagnosticLogger.shared.log("hook.install", fields: ["result": "success"])
            return true
        } catch {
            NSLog("Slipkey: hook install failed: %@", String(describing: error))
            DiagnosticLogger.shared.log("hook.install", fields: [
                "error": String(describing: error),
                "result": "failure"
            ])
            return false
        }
    }

    func stop() {
        hook?.uninstall()
        hook = nil
    }

    func restart(with config: SlipkeyConfig) {
        _ = start(with: config)
    }

    var isRunning: Bool { hook?.isEnabled == true }

    private func handleSwitch(lang: String, config: SlipkeyConfig) {
        guard let entry = config.mappings.first(where: { $0.language.lowercased() == lang.lowercased() }) else {
            NSLog("Slipkey: switch %@ — no mapping configured", lang)
            DiagnosticLogger.shared.log("input_source.mapping_missing", fields: ["language": lang])
            return
        }
        let before = IMEManager.currentSourceID() ?? "<none>"
        DiagnosticLogger.shared.log("input_source.request", fields: Self.currentContextFields().merging([
            "language": lang,
            "requested_source": entry.source,
            "source_before": before
        ]) { _, new in new })
        let result = IMEManager.select(sourceID: entry.source)
        let after = IMEManager.currentSourceID() ?? "<none>"
        switch result {
        case .success:
            NSLog("Slipkey: switch %@: %@ -> %@", lang, before, after)
            DiagnosticLogger.shared.log("input_source.result", fields: Self.currentContextFields().merging([
                "language": lang,
                "requested_source": entry.source,
                "result": "success",
                "source_after": after
            ]) { _, new in new })
            logDelayedState(afterMilliseconds: 50, language: lang, requestedSource: entry.source)
            logDelayedState(afterMilliseconds: 200, language: lang, requestedSource: entry.source)
            logDelayedState(afterMilliseconds: 1000, language: lang, requestedSource: entry.source)
        case .failure(let err):
            NSLog("Slipkey: switch %@ failed: %@ (was: %@)", lang, String(describing: err), before)
            DiagnosticLogger.shared.log("input_source.result", fields: Self.currentContextFields().merging([
                "error": String(describing: err),
                "language": lang,
                "requested_source": entry.source,
                "result": "failure",
                "source_after": after
            ]) { _, new in new })
        }
    }

    private func logDelayedState(afterMilliseconds delay: Int, language: String, requestedSource: String) {
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(delay)) {
            DiagnosticLogger.shared.log("input_source.delayed_state", fields: Self.currentContextFields().merging([
                "delay_ms": String(delay),
                "language": language,
                "requested_source": requestedSource
            ]) { _, new in new })
        }
    }

    private static func currentContextFields() -> [String: String] {
        let application = NSWorkspace.shared.frontmostApplication
        return [
            "frontmost_bundle": application?.bundleIdentifier ?? "<none>",
            "frontmost_pid": application.map { String($0.processIdentifier) } ?? "<none>",
            "source": IMEManager.currentSourceID() ?? "<none>"
        ]
    }
}
