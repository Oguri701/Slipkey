import XCTest
@testable import SlipkeyApp

final class AppLaunchPresentationTests: XCTestCase {
    func test_initial_user_launch_shows_settings_even_after_accessibility_is_granted() {
        XCTAssertTrue(AppLaunchPresentation.shouldShowSettingsOnInitialLaunch(accessibilityTrusted: true))
    }

    func test_initial_launch_without_accessibility_still_shows_settings() {
        XCTAssertTrue(AppLaunchPresentation.shouldShowSettingsOnInitialLaunch(accessibilityTrusted: false))
    }
}
