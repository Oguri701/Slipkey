import AppKit

@MainActor
final class StatusItemManager: NSObject, NSMenuDelegate {
    private let appState: AppState
    private let windowManager: WindowManager
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

    init(appState: AppState, windowManager: WindowManager) {
        self.appState = appState
        self.windowManager = windowManager
        super.init()
        item.button?.image = NSImage(systemSymbolName: "keyboard", accessibilityDescription: "Slipkey")
        item.button?.image?.isTemplate = true
        item.menu = NSMenu()
        item.menu?.delegate = self
    }

    func applyVisibility() {
        if #available(macOS 10.12, *) {
            item.isVisible = appState.menuBarIconVisible
        }
        item.length = appState.menuBarIconVisible ? NSStatusItem.variableLength : 0
    }

    func menuWillOpen(_ menu: NSMenu) {
        menu.removeAllItems()
        if !AccessibilityService.isTrusted {
            menu.addItem(withTitle: L10n.text("Grant Accessibility", appState.uiLanguage), action: #selector(requestAccessibility), keyEquivalent: "")
            menu.items.last?.target = self
            menu.addItem(.separator())
        }
        menu.addItem(withTitle: L10n.text("Preferences", appState.uiLanguage), action: #selector(openPreferences), keyEquivalent: ",")
        menu.items.last?.target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: L10n.text("Quit Slipkey", appState.uiLanguage), action: #selector(quit), keyEquivalent: "q")
        menu.items.last?.target = self
    }

    @objc private func openPreferences() {
        windowManager.showSettings()
    }

    @objc private func requestAccessibility() {
        appState.requestAccessibility()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}
