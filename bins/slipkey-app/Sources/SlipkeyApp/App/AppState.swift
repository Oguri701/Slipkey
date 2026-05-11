import Foundation

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
        accessibilityGranted = trusted
        if trusted && !hook.isRunning {
            _ = hook.start(with: config)
        }
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
        refreshAccessibilityStatus()
    }
}
