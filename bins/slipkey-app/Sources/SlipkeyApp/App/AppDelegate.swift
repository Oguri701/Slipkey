import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private static let showSettingsNotification = Notification.Name("dev.zlb.imeswitch.showSettings")

    private let appState = AppState()
    private lazy var windowManager = WindowManager(appState: appState)
    private lazy var statusItemManager = StatusItemManager(
        appState: appState,
        windowManager: windowManager
    )
    private var isDuplicateInstance = false

    func applicationWillFinishLaunching(_ notification: Notification) {
        if activateExistingInstanceIfNeeded() {
            isDuplicateInstance = true
            NSApp.terminate(nil)
            return
        }

        NSApp.setActivationPolicy(.accessory)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard !isDuplicateInstance else { return }

        let statusItemManager = self.statusItemManager
        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(showSettingsFromDuplicateLaunch),
            name: Self.showSettingsNotification,
            object: nil
        )

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

    func applicationDidBecomeActive(_ notification: Notification) {
        appState.refreshAccessibilityStatus()
    }

    func applicationWillTerminate(_ notification: Notification) {
        DistributedNotificationCenter.default().removeObserver(self)
        appState.hook.stop()
    }

    private func activateExistingInstanceIfNeeded() -> Bool {
        guard let bundleIdentifier = Bundle.main.bundleIdentifier else {
            return false
        }

        let currentPid = ProcessInfo.processInfo.processIdentifier
        guard let existing = NSRunningApplication
            .runningApplications(withBundleIdentifier: bundleIdentifier)
            .first(where: { $0.processIdentifier != currentPid })
        else {
            return false
        }

        DistributedNotificationCenter.default().postNotificationName(
            Self.showSettingsNotification,
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        existing.activate(options: [.activateIgnoringOtherApps])
        return true
    }

    @objc private func showSettingsFromDuplicateLaunch() {
        windowManager.showSettings()
    }
}
