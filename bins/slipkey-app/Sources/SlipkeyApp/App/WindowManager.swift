import AppKit
import Combine
import SwiftUI

@MainActor
final class WindowManager: NSObject, NSWindowDelegate, NSToolbarDelegate {
    private let appState: AppState
    private let tabState = SettingsTabState()
    private var settingsWindow: NSWindow?
    private var cancellables: Set<AnyCancellable> = []
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

        tabState.$selection
            .dropFirst()
            .sink { [weak self, weak window] section in
                toolbar.selectedItemIdentifier = NSToolbarItem.Identifier(section.rawValue)
                self?.scheduleResize(window: window, animated: true)
            }
            .store(in: &cancellables)

        appState.objectWillChange.sink { [weak self, weak window] in
            DispatchQueue.main.async {
                guard let self else { return }
                self.refreshToolbarLabels(in: window?.toolbar)
                self.scheduleResize(window: window, animated: true)
            }
        }
        .store(in: &cancellables)

        scheduleResize(window: window, animated: false)
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

    private func scheduleResize(window: NSWindow?, animated: Bool) {
        guard let window else { return }
        DispatchQueue.main.async { [weak self, weak window] in
            guard let self, let window else { return }
            let contentHeight = SettingsContentFitter.contentHeight(
                for: self.appState,
                section: self.tabState.selection,
                width: window.contentView?.bounds.width ?? window.contentLayoutRect.width
            )
            self.applyContentHeight(contentHeight, to: window, animated: animated)
        }
    }

    private func applyContentHeight(_ height: CGFloat, to window: NSWindow, animated: Bool) {
        guard height > 10 else { return }
        let clamped = Self.clampToScreen(height)
        if abs(clamped - lastAppliedContentHeight) < 1.0 { return }
        lastAppliedContentHeight = clamped

        if isInitialDisplay || !animated {
            isInitialDisplay = false
            snapHeight(window: window, to: clamped)
        } else {
            animate(window: window, toContentHeight: clamped)
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
}

@MainActor
enum SettingsContentFitter {
    static func contentHeight(for appState: AppState, section: SettingsSection, width: CGFloat) -> CGFloat {
        let measuringWidth = max(width, 450)
        let host = NSHostingView(
            rootView: SettingsTabContent(appState: appState, selection: section, refreshOnAppear: false)
                .fixedSize(horizontal: false, vertical: true)
                .frame(width: measuringWidth, alignment: .top)
        )
        host.setFrameSize(NSSize(width: measuringWidth, height: 1))
        host.layoutSubtreeIfNeeded()
        return ceil(host.fittingSize.height)
    }
}
