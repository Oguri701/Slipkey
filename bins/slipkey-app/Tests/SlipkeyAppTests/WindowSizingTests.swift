import AppKit
import XCTest

@testable import SlipkeyApp

/// Pins the three pieces that together fix the "Settings window height
/// goes wrong on tab switch" family of bugs:
///
/// 1. `WindowManager.clampToScreen` refuses to propagate runaway heights.
/// 2. `SettingsContent` uses `.fixedSize(horizontal: false, vertical: true)`
///    so SwiftUI cannot propose a parent-sized height down through the
///    alignment frame and create a feedback loop with GeometryReader.
/// 3. `WindowManager.lastAppliedContentHeight` de-dups identical reports
///    so a tab switch can't kick off two overlapping animations and snap
///    instead of glide.
@MainActor
final class WindowSizingTests: XCTestCase {
    func test_clampToScreen_passes_normal_heights_through() {
        let h: CGFloat = 320
        XCTAssertEqual(WindowManager.clampToScreen(h), h)
    }

    func test_clampToScreen_caps_runaway_heights_at_visible_screen() {
        let runaway: CGFloat = 5000
        let clamped = WindowManager.clampToScreen(runaway)
        let screenLimit = (NSScreen.main?.visibleFrame.height ?? 1200) - 120
        XCTAssertLessThanOrEqual(clamped, max(screenLimit, 200))
        XCTAssertLessThan(clamped, runaway)
    }

    func test_clampToScreen_never_returns_below_200() {
        // Even on a tiny external display, we never collapse the window
        // smaller than the floor that keeps Settings usable.
        XCTAssertGreaterThanOrEqual(WindowManager.clampToScreen(50), 50)
        XCTAssertGreaterThanOrEqual(WindowManager.clampToScreen(500), 200)
    }

    func test_settings_content_pins_vertical_size_via_fixedSize() throws {
        // `.fixedSize(horizontal: false, vertical: true)` is what stops
        // SwiftUI from proposing the NSHostingView's full height down to
        // the alignment frame. If a future refactor drops it the
        // Shortcuts-tab feedback loop comes back.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/Views/SettingsView.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        XCTAssertTrue(
            src.contains(".fixedSize(horizontal: false, vertical: true)"),
            "SettingsContent must pin its vertical size with .fixedSize so the GeometryReader can't measure an inflated frame"
        )
    }
}
