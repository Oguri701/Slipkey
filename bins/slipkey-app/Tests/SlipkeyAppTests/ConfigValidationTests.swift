import XCTest
@testable import SlipkeyApp

final class ConfigValidationTests: XCTestCase {
    func testValidDefaultPrefixesPassValidation() {
        XCTAssertTrue(SlipkeyConfig.defaults().validationErrors().isEmpty)
    }

    func testRejectsUnsupportedPrefixCharacters() {
        let config = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "en", prefix: "e n", source: "com.apple.keylayout.ABC"),
                MappingEntry(language: "ja", prefix: "日本", source: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese")
            ]
        )

        XCTAssertFalse(config.validationErrors().isEmpty)
    }

    func testRejectsDuplicatePrefixes() {
        let config = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "en", prefix: "en", source: "com.apple.keylayout.ABC"),
                MappingEntry(language: "ja", prefix: "EN", source: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese")
            ]
        )

        XCTAssertFalse(config.validationErrors().isEmpty)
    }

    func testRejectsPrefixOfAnotherPrefix() {
        let config = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "en", prefix: "e", source: "com.apple.keylayout.ABC"),
                MappingEntry(language: "ja", prefix: "en", source: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese")
            ]
        )

        XCTAssertFalse(config.validationErrors().isEmpty)
    }
}
