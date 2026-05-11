import AppKit
import Combine
import SwiftUI

@MainActor
final class WindowManager: NSObject, NSWindowDelegate, NSToolbarDelegate {
    private let appState: AppState
    private let tabState = SettingsTabState()
    private var settingsWindow: NSWindow?
    private var languageObserver: AnyCancellable?
    private var isInitialDisplay = true
    /// Last height we already animated to. SwiftUI may report the same
    /// height several times per layout pass (especially on a tab switch
    /// that fires `onAppear`); without de-duplication every report would
    /// kick off a fresh `NSAnimationContext.runAnimationGroup`, cancel the
    /// previous one mid-flight, and the user would see the window snap
    /// instead of glide.
    private var lastAppliedContentHeight: CGFloat = -1

    init(appState: AppState) {
        self.appState = appState
        super.init()
    }

    func showSettings() {
        if settingsWindow == nil {
            isInitialDisplay = true
            settingsWindow = makeSettingsWindow()
        }
        NSApp.setActivationPolicy(.regular)
        settingsWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func makeSettingsWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 450, height: 260),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "Slipkey"
        window.isReleasedWhenClosed = false
        window.delegate = self

        window.contentView = NSHostingView(rootView: SettingsContent(appState: appState, tabState: tabState))

        let toolbar = NSToolbar(identifier: "SlipkeySettingsToolbar")
        toolbar.delegate = self
        toolbar.displayMode = .iconAndLabel
        toolbar.allowsUserCustomization = false
        toolbar.selectedItemIdentifier = NSToolbarItem.Identifier(tabState.selection.rawValue)
        window.toolbar = toolbar
        window.toolbarStyle = .preference

        window.center()

        tabState.onContentHeight = { [weak self, weak window] height in
            guard let self, let window, height > 10 else { return }
            // Safety net: if SwiftUI ever reports an inflated content
            // height (we hit a feedback loop in 0.1.1 where the height
            // grew every Shortcuts repaint), refuse to grow the window
            // beyond the visible screen area.
            let clamped = Self.clampToScreen(height)
            // De-dup repeated reports of the same height so a tab switch
            // can't kick off two overlapping animations and snap the
            // window instead of gliding it.
            if abs(clamped - self.lastAppliedContentHeight) < 1.0 { return }
            self.lastAppliedContentHeight = clamped
            if self.isInitialDisplay {
                self.isInitialDisplay = false
                self.snapHeight(window: window, to: clamped)
            } else {
                self.animate(window: window, toContentHeight: clamped)
            }
        }

        languageObserver = appState.objectWillChange.sink { [weak self, weak window] in
            DispatchQueue.main.async {
                self?.refreshToolbarLabels(in: window?.toolbar)
            }
        }

        return window
    }

    /// Clamp a reported content height so a runaway SwiftUI measurement
    /// can't push the Settings window taller than the screen the user is
    /// looking at. 120 pt of headroom keeps the title bar and Dock visible.
    static func clampToScreen(_ height: CGFloat) -> CGFloat {
        let screenLimit = (NSScreen.main?.visibleFrame.height ?? 1200) - 120
        return min(height, max(screenLimit, 200))
    }

    private func snapHeight(window: NSWindow, to contentHeight: CGFloat) {
        let titleBarH = window.frame.height - (window.contentView?.frame.height ?? 0)
        let newH = contentHeight + titleBarH
        let newY = window.frame.maxY - newH
        window.setFrame(NSRect(x: window.frame.minX, y: newY, width: window.frame.width, height: newH), display: true)
    }

    private func animate(window: NSWindow, toContentHeight contentHeight: CGFloat) {
        let titleBarH = window.frame.height - (window.contentView?.frame.height ?? 0)
        let oldFrame = window.frame
        let newSize = NSSize(width: oldFrame.width, height: contentHeight + titleBarH)
        let newOrigin = NSPoint(x: oldFrame.minX, y: oldFrame.maxY - newSize.height)
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = 0.22
            ctx.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            window.animator().setFrame(NSRect(origin: newOrigin, size: newSize), display: true)
        }
    }

    private func refreshToolbarLabels(in toolbar: NSToolbar?) {
        guard let toolbar else { return }
        for item in toolbar.items {
            if let section = SettingsSection(rawValue: item.itemIdentifier.rawValue) {
                item.label = section.title(appState.uiLanguage)
            }
        }
    }

    // MARK: - NSToolbarDelegate

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        SettingsSection.allCases.map { NSToolbarItem.Identifier($0.rawValue) }
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbarSelectableItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(_ toolbar: NSToolbar,
                 itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
                 willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        guard let section = SettingsSection(rawValue: itemIdentifier.rawValue) else { return nil }
        let item = NSToolbarItem(itemIdentifier: itemIdentifier)
        item.label = section.title(appState.uiLanguage)
        item.image = NSImage(systemSymbolName: section.systemImage, accessibilityDescription: nil)
        item.action = #selector(selectTab(_:))
        item.target = self
        return item
    }

    @objc private func selectTab(_ sender: NSToolbarItem) {
        guard let section = SettingsSection(rawValue: sender.itemIdentifier.rawValue) else { return }
        tabState.selection = section
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        isInitialDisplay = true
        lastAppliedContentHeight = -1
    }
}

@MainActor
final class SettingsTabState: ObservableObject {
    @Published var selection: SettingsSection = .general
    var onContentHeight: ((CGFloat) -> Void)?
}
