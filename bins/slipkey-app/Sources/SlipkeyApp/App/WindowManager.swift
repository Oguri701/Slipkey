import AppKit
import SwiftUI

@MainActor
final class WindowManager: NSObject, NSWindowDelegate {
    static weak var shared: WindowManager?

    private let appState: AppState
    private var settingsWindow: NSWindow?

    init(appState: AppState) {
        self.appState = appState
        super.init()
        WindowManager.shared = self
    }

    func showSettings() {
        if settingsWindow == nil {
            let content = SettingsView(appState: appState)
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 560, height: 380),
                styleMask: [.titled, .closable, .miniaturizable],
                backing: .buffered,
                defer: false
            )
            window.title = "Slipkey"
            window.contentView = NSHostingView(rootView: content)
            window.delegate = self
            window.isReleasedWhenClosed = false
            window.center()
            settingsWindow = window
        }

        NSApp.setActivationPolicy(.regular)
        settingsWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}
