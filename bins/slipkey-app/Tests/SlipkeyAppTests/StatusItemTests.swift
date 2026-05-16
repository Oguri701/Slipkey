import XCTest
@testable import SlipkeyApp

@MainActor
final class StatusItemTests: XCTestCase {
    func test_status_item_uses_square_length_when_visible() {
        XCTAssertEqual(StatusItemManager.statusItemLength(isVisible: true), 28)
    }

    func test_status_item_collapses_when_hidden_by_user_setting() {
        XCTAssertEqual(StatusItemManager.statusItemLength(isVisible: false), 0)
    }

    func test_status_item_image_has_fallback() {
        XCTAssertNotNil(StatusItemManager.makeStatusImage())
    }
}
