import XCTest
@testable import SlipkeyApp

final class EventHookTests: XCTestCase {
    func test_composition_probe_only_runs_for_idle_leader_key() {
        XCTAssertTrue(EventHook.shouldInspectComposition(idle: true, key: .leader))
        XCTAssertFalse(EventHook.shouldInspectComposition(idle: true, key: .alphaNum("e")))
        XCTAssertFalse(EventHook.shouldInspectComposition(idle: true, key: .other))
        XCTAssertFalse(EventHook.shouldInspectComposition(idle: false, key: .leader))
    }

    func test_hook_service_running_state_checks_event_tap_health() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/Services/HookService.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        XCTAssertFalse(src.contains("var isRunning: Bool { hook != nil }"))
        XCTAssertTrue(src.contains("hook?.isEnabled == true"))
    }
}
