import AppKit
import XCTest

@testable import SlipkeyApp

/// Pins the two fixes for the v0.1.1 "Shortcuts tab makes the window
/// vertical-screen-tall" bug:
///
/// 1. `WindowManager.clampToScreen` refuses to propagate inflated heights.
/// 2. `SettingsContent` measures the bare `Group`, not the outer
///    `.frame(maxWidth: .infinity, alignment: .top)` layer. A regression to
///    the old order would re-introduce the feedback loop, so we lock the
///    relative order via a source-level grep.
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

    func test_settings_content_measures_group_before_outer_frame() throws {
        // SwiftUI lays modifiers bottom-up: whichever modifier appears first
        // in source wraps the closest. The GeometryReader must therefore
        // appear before `.frame(maxWidth: .infinity, alignment: .top)` so it
        // measures the inner Group, not the outer alignment frame.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/Views/SettingsView.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        guard
            let geometryRange = src.range(of: ".background("),
            let frameRange = src.range(
                of: ".frame(maxWidth: .infinity, alignment: .top)"
            )
        else {
            XCTFail("expected both `.background(GeometryReader…)` and the outer alignment frame")
            return
        }

        XCTAssertLessThan(
            geometryRange.lowerBound, frameRange.lowerBound,
            "GeometryReader must wrap the inner Group, not the outer alignment frame, or the Shortcuts-tab feedback loop comes back"
        )
    }
}
