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
        item.button?.image = Self.makeStatusImage()
        item.button?.image?.isTemplate = true
        item.menu = NSMenu()
        item.menu?.delegate = self
    }

    func applyVisibility() {
        if #available(macOS 10.12, *) {
            item.isVisible = appState.menuBarIconVisible
        }
        item.length = Self.statusItemLength(isVisible: appState.menuBarIconVisible)
    }

    static func statusItemLength(isVisible: Bool) -> CGFloat {
        isVisible ? 28 : 0
    }

    static func makeStatusImage() -> NSImage? {
        if let url = Bundle.main.url(forResource: "status-keyboard-template", withExtension: "png"),
           let image = NSImage(contentsOf: url) {
            image.size = NSSize(width: 21, height: 21)
            image.isTemplate = true
            return image
        }

        return drawFallbackKeyboardImage()
    }

    private static func drawFallbackKeyboardImage() -> NSImage {
        let image = NSImage(size: NSSize(width: 18, height: 18))
        image.lockFocus()

        NSColor.black.setStroke()
        NSColor.black.setFill()

        let body = NSBezierPath(roundedRect: NSRect(x: 2.5, y: 4.5, width: 13, height: 9), xRadius: 2, yRadius: 2)
        body.lineWidth = 1.2
        body.stroke()

        for y in [10.0, 7.0] {
            for x in [5.0, 8.0, 11.0] {
                NSBezierPath(rect: NSRect(x: x, y: y, width: 1.5, height: 1.5)).fill()
            }
        }

        image.unlockFocus()
        image.isTemplate = true
        return image
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
