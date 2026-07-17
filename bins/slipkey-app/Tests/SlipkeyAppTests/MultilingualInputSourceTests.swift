import XCTest
@testable import SlipkeyApp

final class MultilingualInputSourceTests: XCTestCase {
    func test_detection_queries_only_currently_enabled_system_sources() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/SlipkeyApp/Hook/IMEManager.swift")
        let source = try String(contentsOf: url, encoding: .utf8)

        XCTAssertTrue(source.contains("TISCreateInputSourceList(nil, false)"))
        XCTAssertFalse(source.contains("TISCreateInputSourceList(nil, true)"))
    }

    func test_macos_language_tags_normalize_to_supported_language_codes() {
        XCTAssertEqual(InputSourceService.normalizedLanguage("en-US"), "en")
        XCTAssertEqual(InputSourceService.normalizedLanguage("ja-JP"), "ja")
        XCTAssertEqual(InputSourceService.normalizedLanguage("zh-Hans-CN"), "zh")
        XCTAssertEqual(InputSourceService.normalizedLanguage("ko-KR"), "ko")
        XCTAssertEqual(InputSourceService.normalizedLanguage("fr-FR"), "fr")
        XCTAssertEqual(InputSourceService.normalizedLanguage("de-DE"), "de")
        XCTAssertEqual(InputSourceService.normalizedLanguage("es-ES"), "es")
        XCTAssertNil(InputSourceService.normalizedLanguage("und"))
    }

    func test_detection_merge_creates_one_row_per_language_and_keeps_multiple_sources_grouped() {
        let config = SlipkeyConfig.defaults().mergingDetectedSources([
            InputSource(language: "en", sourceID: "com.apple.keylayout.ABC", name: "ABC", rawLanguage: "en-US", isSelectable: true),
            InputSource(language: "ja", sourceID: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese", name: "Japanese - Romaji", rawLanguage: "ja-JP", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.ITABC", name: "Pinyin", rawLanguage: "zh-Hans", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.Shuangpin", name: "Shuangpin", rawLanguage: "zh-Hans", isSelectable: true),
            InputSource(language: "ko", sourceID: "com.apple.inputmethod.Korean.2SetKorean", name: "Korean", rawLanguage: "ko-KR", isSelectable: true),
            InputSource(language: "fr", sourceID: "com.apple.keylayout.French", name: "French", rawLanguage: "fr-FR", isSelectable: true)
        ])

        XCTAssertEqual(config.mappings.map(\.language), ["en", "fr", "ja", "ko", "zh"])
        XCTAssertEqual(config.mappings.first { $0.language == "fr" }?.prefix, "fr")
        XCTAssertEqual(config.mappings.first { $0.language == "ko" }?.prefix, "ko")
        XCTAssertEqual(config.mappings.filter { $0.language == "zh" }.count, 1)
    }

    func test_detection_preserves_custom_prefix_and_selected_source() {
        let existing = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "zh", prefix: "cn", source: "com.apple.inputmethod.SCIM.Shuangpin", name: "Shuangpin", enabled: true)
            ]
        )

        let merged = existing.mergingDetectedSources([
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.ITABC", name: "Pinyin", rawLanguage: "zh-Hans", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.Shuangpin", name: "Shuangpin", rawLanguage: "zh-Hans", isSelectable: true)
        ])

        XCTAssertEqual(merged.mappings.first?.prefix, "cn")
        XCTAssertEqual(merged.mappings.first?.source, "com.apple.inputmethod.SCIM.Shuangpin")
        XCTAssertEqual(merged.mappings.first?.name, "Shuangpin")
    }

    func test_detection_removes_languages_missing_from_system_sources() {
        let existing = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "fr", prefix: "ff", source: "missing.french.source", name: "Old French", enabled: false)
            ]
        )

        let merged = existing.mergingDetectedSources([
            InputSource(language: "en", sourceID: "com.apple.keylayout.ABC", name: "ABC", rawLanguage: "en-US", isSelectable: true)
        ])

        XCTAssertEqual(merged.mappings.map(\.language), ["en"])
        XCTAssertNil(merged.mappings.first { $0.language == "fr" })
    }

    func test_readded_language_uses_defaults_instead_of_removed_configuration() {
        let existing = SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "ko", prefix: "old", source: "missing.korean.source", name: "Old Korean", enabled: false)
            ]
        )
        let english = InputSource(
            language: "en",
            sourceID: "com.apple.keylayout.ABC",
            name: "ABC",
            rawLanguage: "en-US",
            isSelectable: true
        )
        let afterRemoval = existing.mergingDetectedSources([english])
        let readded = afterRemoval.mergingDetectedSources([
            english,
            InputSource(
                language: "ko",
                sourceID: "com.apple.inputmethod.Korean.2SetKorean",
                name: "2-Set Korean",
                rawLanguage: "ko-KR",
                isSelectable: true
            )
        ])

        let korean = readded.mappings.first { $0.language == "ko" }
        XCTAssertEqual(korean?.prefix, "ko")
        XCTAssertEqual(korean?.source, "com.apple.inputmethod.Korean.2SetKorean")
        XCTAssertEqual(korean?.name, "2-Set Korean")
        XCTAssertEqual(korean?.enabled, true)
    }

    func test_detection_dedupes_same_language_same_display_name_but_keeps_distinct_chinese_sources() {
        let sources = InputSourceService.dedupedForDisplay([
            InputSource(language: "ja", sourceID: "com.apple.inputmethod.Kotoeri.KanaTyping.Japanese", name: "Hiragana", rawLanguage: "ja", isSelectable: true),
            InputSource(language: "ja", sourceID: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese", name: "Hiragana", rawLanguage: "ja", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.Shuangpin", name: "Shuangpin - Simplified", rawLanguage: "zh-Hans", isSelectable: true),
            InputSource(language: "zh", sourceID: "com.apple.inputmethod.SCIM.ITABC", name: "Pinyin - Simplified", rawLanguage: "zh-Hans", isSelectable: true)
        ])

        XCTAssertEqual(sources.filter { $0.language == "ja" }.count, 1)
        XCTAssertEqual(sources.first { $0.language == "ja" }?.sourceID, "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese")
        XCTAssertEqual(sources.filter { $0.language == "zh" }.count, 2)
    }
}
