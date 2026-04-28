import AppKit
import Combine
import SwiftUI

@MainActor
final class WindowManager: NSObject, NSWindowDelegate, NSToolbarDelegate {
    static weak var shared: WindowManager?

    private let appState: AppState
    private let tabState = SettingsTabState()
    private var settingsWindow: NSWindow?
    private var languageObserver: AnyCancellable?
    private var isInitialDisplay = true

    init(appState: AppState) {
        self.appState = appState
        super.init()
        WindowManager.shared = self
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
            if self.isInitialDisplay {
                self.isInitialDisplay = false
                self.snapHeight(window: window, to: height)
            } else {
                self.animate(window: window, toContentHeight: height)
            }
        }

        languageObserver = appState.objectWillChange.sink { [weak self, weak window] in
            DispatchQueue.main.async {
                self?.refreshToolbarLabels(in: window?.toolbar)
            }
        }

        return window
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
    }
}

@MainActor
final class SettingsTabState: ObservableObject {
    @Published var selection: SettingsSection = .general
    var onContentHeight: ((CGFloat) -> Void)?
}
