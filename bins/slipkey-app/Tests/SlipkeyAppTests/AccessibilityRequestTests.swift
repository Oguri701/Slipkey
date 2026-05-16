import XCTest

final class AccessibilityRequestTests: XCTestCase {
    private var appRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
    }

    func test_accessibility_request_uses_system_prompt_without_opening_settings() throws {
        let src = try String(
            contentsOf: appRoot.appendingPathComponent("Sources/SlipkeyApp/Services/AccessibilityService.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(src.contains("AXIsProcessTrustedWithOptions"))
        XCTAssertTrue(src.contains("kAXTrustedCheckOptionPrompt"))
        XCTAssertFalse(src.contains("NSWorkspace.shared.open"))
        XCTAssertFalse(src.contains("x-apple.systempreferences"))
    }
}
