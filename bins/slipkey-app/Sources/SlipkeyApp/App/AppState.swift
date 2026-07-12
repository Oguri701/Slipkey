import Foundation

enum AccessibilityMonitorAction: Equatable {
    case none
    case startHook
    case stopHook

    static func resolve(wasGranted: Bool, isTrusted: Bool, hookRunning: Bool) -> AccessibilityMonitorAction {
        if !isTrusted && (wasGranted || hookRunning) {
            return .stopHook
        }
        if isTrusted && !hookRunning {
            return .startHook
        }
        return .none
    }

    static func shouldContinueMonitoring(isTrusted: Bool, hookRunning: Bool) -> Bool {
        !isTrusted || !hookRunning
    }
}

@MainActor
final class AppState: ObservableObject {
    @Published var config = SlipkeyConfig.defaults()
    @Published var detectedSources: [InputSource] = []
    @Published var statusMessage = ""
    @Published var accessibilityGranted = AccessibilityService.isTrusted

    let hook = HookService()
    let settings = SettingsStore()
    var menuBarIconVisibilityDidChange: ((Bool) -> Void)?
    private let inputSourceService = InputSourceService()
    private var accessibilityMonitorTask: Task<Void, Never>?

    var menuBarIconVisible: Bool {
        get { settings.menuBarIconVisible }
        set {
            guard settings.menuBarIconVisible != newValue else { return }
            settings.menuBarIconVisible = newValue
            objectWillChange.send()
            menuBarIconVisibilityDidChange?(newValue)
        }
    }

    var launchAtLogin: Bool {
        get { settings.launchAtLogin }
        set {
            settings.launchAtLogin = newValue
            objectWillChange.send()
            LoginItemService.setEnabled(newValue)
        }
    }

    var uiLanguage: String {
        get { settings.uiLanguage }
        set {
            settings.uiLanguage = newValue
            objectWillChange.send()
        }
    }

    func load() {
        config = ConfigStore.load()
        refreshDetectedSources()
        accessibilityGranted = AccessibilityService.isTrusted
        launchAtLogin = LoginItemService.isEnabled
    }

    func refreshAccessibilityStatus() {
        let trusted = AccessibilityService.isTrusted
        let action = AccessibilityMonitorAction.resolve(
            wasGranted: accessibilityGranted,
            isTrusted: trusted,
            hookRunning: hook.isRunning
        )
        accessibilityGranted = trusted

        switch action {
        case .startHook:
            _ = hook.start(with: config)
            statusMessage = L10n.text("Saved. Shortcuts are active now.", uiLanguage)
        case .stopHook:
            hook.stop()
            statusMessage = L10n.text("Accessibility permission required", uiLanguage)
        case .none:
            break
        }
    }

    func startAccessibilityPermissionMonitor() {
        accessibilityMonitorTask?.cancel()
        refreshAccessibilityStatus()

        accessibilityMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                guard !Task.isCancelled else { return }
                guard let self else { return }

                self.refreshAccessibilityStatus()
                guard AccessibilityMonitorAction.shouldContinueMonitoring(
                    isTrusted: self.accessibilityGranted,
                    hookRunning: self.hook.isRunning
                ) else { return }
                try? await Task.sleep(nanoseconds: 500_000_000)
            }
        }
    }

    func stopAccessibilityPermissionMonitor() {
        accessibilityMonitorTask?.cancel()
        accessibilityMonitorTask = nil
    }

    func refreshDetectedSources() {
        detectedSources = inputSourceService.discover()
        if detectedSources.isEmpty {
            return
        }
        config = config.mergingDetectedSources(detectedSources)
    }

    func saveAndRestart() {
        let errors = config.validationErrors()
        guard errors.isEmpty else {
            statusMessage = L10n.text(errors[0], uiLanguage)
            return
        }
        do {
            try ConfigStore.save(config)
            hook.restart(with: config)
            statusMessage = L10n.text("Saved. Shortcuts are active now.", uiLanguage)
        } catch {
            statusMessage = error.localizedDescription
        }
    }

    func resetShortcutsToDefaults() {
        config = SlipkeyConfig.defaults().mergingDetectedSources(detectedSources)
        statusMessage = L10n.text("Defaults restored. Click Save to apply.", uiLanguage)
    }

    func requestAccessibility() {
        AccessibilityService.request()
        startAccessibilityPermissionMonitor()
    }
}
