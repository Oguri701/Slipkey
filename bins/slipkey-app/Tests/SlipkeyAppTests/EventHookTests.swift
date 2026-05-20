import XCTest
import CoreGraphics
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

    func test_hook_service_ignores_disabled_mapping_rows() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // SlipkeyAppTests
            .deletingLastPathComponent() // Tests
            .deletingLastPathComponent() // slipkey-app
            .appendingPathComponent("Sources/SlipkeyApp/Services/HookService.swift")
        let src = try String(contentsOf: url, encoding: .utf8)

        XCTAssertTrue(src.contains(".filter { $0.enabled && !$0.prefix.isEmpty }"))
    }

    func test_synthetic_replay_marker_is_detected() throws {
        guard let event = CGEvent(keyboardEventSource: nil, virtualKey: Keycode.semicolon, keyDown: true) else {
            XCTFail("CGEvent should be creatable in tests")
            return
        }

        XCTAssertFalse(EventHook.isSyntheticReplayEvent(event))
        event.setIntegerValueField(.eventSourceUserData, value: 0x534c_4950_4b45_5901)
        XCTAssertTrue(EventHook.isSyntheticReplayEvent(event))
    }

    func test_async_replay_includes_current_key_for_cancelled_alpha_sequence() {
        let response = StateMachineResponse.cancel(replay: [.leader, .alphaNum("j")])
        let keys = EventHook.replayKeys(
            for: response,
            currentKey: .alphaNum("r"),
            currentKeycode: 0x0F,
            leaderKeycode: Keycode.semicolon
        )

        XCTAssertEqual(keys, [Keycode.semicolon, 0x26, 0x0F])
        XCTAssertTrue(EventHook.shouldSuppressCurrentEventForAsyncReplay(response: response, currentKey: .alphaNum("r")))
    }

    func test_async_replay_does_not_suppress_unmappable_current_key() {
        let response = StateMachineResponse.cancel(replay: [.leader])

        XCTAssertFalse(EventHook.shouldSuppressCurrentEventForAsyncReplay(response: response, currentKey: .other))
        XCTAssertEqual(
            EventHook.replayKeys(
                for: response,
                currentKey: .other,
                currentKeycode: 0,
                leaderKeycode: Keycode.semicolon
            ),
            [Keycode.semicolon]
        )
    }

    func test_switch_response_has_no_replay_keys() {
        let response = StateMachineResponse.switchTo("ja")
        XCTAssertEqual(
            EventHook.replayKeys(
                for: response,
                currentKey: .alphaNum("a"),
                currentKeycode: 0x00,
                leaderKeycode: Keycode.semicolon
            ),
            []
        )
    }
}
