import Foundation

@MainActor
final class HookService {
    private var hook: EventHook?

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
            return true
        } catch {
            NSLog("Slipkey: hook install failed: %@", String(describing: error))
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
            return
        }
        let before = IMEManager.currentSourceID() ?? "<none>"
        let result = IMEManager.select(sourceID: entry.source)
        let after = IMEManager.currentSourceID() ?? "<none>"
        switch result {
        case .success:
            NSLog("Slipkey: switch %@: %@ -> %@", lang, before, after)
        case .failure(let err):
            NSLog("Slipkey: switch %@ failed: %@ (was: %@)", lang, String(describing: err), before)
        }
    }
}
