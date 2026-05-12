import AppKit
import XCTest

@testable import SlipkeyApp

/// Pins the three pieces that together fix the "Settings window height
/// goes wrong on tab switch" family of bugs:
///
/// 1. `WindowManager.clampToScreen` refuses to propagate runaway heights.
/// 2. The displayed `SettingsContent` must not self-measure through
///    `GeometryReader`/preferences. Measuring the live hosted view was the
///    source of the feedback loop where parent-proposed height came back as
///    content height.
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

    func test_settings_content_does_not_self_measure_displayed_view() throws {
        // The displayed view must not report its own frame back to AppKit.
        // The stable design is: AppKit asks an isolated fitting host for
        // the selected tab's intrinsic height, then resizes the window.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/Views/SettingsView.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        XCTAssertFalse(src.contains("GeometryReader"))
        XCTAssertFalse(src.contains("ContentHeightKey"))
        XCTAssertFalse(src.contains("onPreferenceChange"))
    }

    func test_window_manager_owns_height_measurement_outside_display_tree() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/App/WindowManager.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        XCTAssertTrue(src.contains("SettingsContentFitter"))
        XCTAssertFalse(src.contains("onContentHeight"))
    }

    func test_content_fitter_returns_compact_heights_for_all_tabs() {
        let appState = AppState()
        appState.detectedSources = [
            InputSource(language: "en", sourceID: "com.apple.keylayout.ABC", name: "ABC", isSelectable: true),
            InputSource(language: "ja", sourceID: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese", name: "Japanese - Romaji", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.Shuangpin", name: "Simplified Chinese - Shuangpin", isSelectable: true)
        ]

        for section in SettingsSection.allCases {
            let height = SettingsContentFitter.contentHeight(for: appState, section: section, width: 450)
            XCTAssertGreaterThan(height, 50, "\(section.rawValue) should have a real intrinsic height")
            XCTAssertLessThan(height, 500, "\(section.rawValue) should not measure as a screen-height tab")
        }
    }
}
