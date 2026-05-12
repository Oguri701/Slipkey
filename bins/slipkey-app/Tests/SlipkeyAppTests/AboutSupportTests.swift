import XCTest

@MainActor
final class AboutSupportTests: XCTestCase {
    private var appRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
    }

    private var repoRoot: URL {
        appRoot
            .deletingLastPathComponent() // bins
            .deletingLastPathComponent() // repo root
    }

    func test_wechat_support_qr_is_bundled_with_app_resources() {
        let resource = appRoot.appendingPathComponent("Resources/wechat-support.jpeg")
        XCTAssertTrue(FileManager.default.fileExists(atPath: resource.path))
    }

    func test_about_tab_exposes_native_support_sheet() throws {
        let src = try String(
            contentsOf: appRoot.appendingPathComponent("Sources/SlipkeyApp/Views/SettingsView.swift"),
            encoding: .utf8
        )

        XCTAssertTrue(src.contains("Support author"))
        XCTAssertTrue(src.contains(".sheet(isPresented: $showingSupportSheet)"))
        XCTAssertTrue(src.contains("SupportAuthorSheet"))
        XCTAssertTrue(src.contains("wechat-support"))
    }

    func test_macos_packaging_copies_support_qr_resource() throws {
        let script = try String(
            contentsOf: repoRoot.appendingPathComponent("scripts/package-macos.sh"),
            encoding: .utf8
        )
        let workflow = try String(
            contentsOf: repoRoot.appendingPathComponent(".github/workflows/release.yml"),
            encoding: .utf8
        )

        XCTAssertTrue(script.contains("wechat-support.jpeg"))
        XCTAssertTrue(workflow.contains("wechat-support.jpeg"))
    }
}
