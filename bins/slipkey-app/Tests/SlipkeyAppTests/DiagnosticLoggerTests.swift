import XCTest
@testable import SlipkeyApp

final class DiagnosticLoggerTests: XCTestCase {
    func test_log_format_is_single_line_and_quotes_field_values() {
        let line = DiagnosticLogger.formatLine(
            timestamp: "2026-07-12T12:00:00.000Z",
            event: "input source.result",
            fields: ["frontmost_bundle": "com.example.Editor", "error": "line 1\nline 2"]
        )

        XCTAssertEqual(line.filter { $0 == "\n" }.count, 1)
        XCTAssertTrue(line.contains("input_source.result"))
        XCTAssertTrue(line.contains("frontmost_bundle=\"com.example.Editor\""))
        XCTAssertTrue(line.contains("error=\"line 1 line 2\""))
    }

    func test_diagnostics_use_a_bounded_dedicated_log_file() {
        XCTAssertTrue(DiagnosticLogger.logURL.path.hasSuffix("Library/Logs/Slipkey/diagnostics.log"))
    }
}
