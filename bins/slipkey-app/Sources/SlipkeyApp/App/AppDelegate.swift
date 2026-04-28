import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let appState = AppState()
    private lazy var statusItemManager = StatusItemManager(appState: appState)
    private lazy var windowManager = WindowManager(appState: appState)

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let statusItemManager = self.statusItemManager
        // Force windowManager to instantiate so WindowManager.shared is set —
        // status-item "Preferences" goes through the static shared reference.
        _ = self.windowManager

        appState.menuBarIconVisibilityDidChange = { [weak statusItemManager] _ in
            statusItemManager?.applyVisibility()
        }
        appState.load()
        statusItemManager.applyVisibility()
        _ = appState.hook.start(with: appState.config)

        if !AccessibilityService.isTrusted {
            windowManager.showSettings()
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            windowManager.showSettings()
            return false
        }
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        appState.hook.stop()
    }
}
