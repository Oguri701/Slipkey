import XCTest
@testable import SlipkeyApp

final class AccessibilityMonitorTests: XCTestCase {
    func test_monitor_only_continues_while_permission_or_hook_is_unavailable() {
        XCTAssertTrue(AccessibilityMonitorAction.shouldContinueMonitoring(isTrusted: false, hookRunning: false))
        XCTAssertTrue(AccessibilityMonitorAction.shouldContinueMonitoring(isTrusted: true, hookRunning: false))
        XCTAssertFalse(AccessibilityMonitorAction.shouldContinueMonitoring(isTrusted: true, hookRunning: true))
    }

    func test_revoked_permission_stops_running_hook() {
        XCTAssertEqual(
            AccessibilityMonitorAction.resolve(wasGranted: true, isTrusted: false, hookRunning: true),
            .stopHook
        )
    }

    func test_restored_permission_starts_hook_when_not_running() {
        XCTAssertEqual(
            AccessibilityMonitorAction.resolve(wasGranted: false, isTrusted: true, hookRunning: false),
            .startHook
        )
    }

    func test_granted_permission_restarts_unhealthy_hook_even_if_permission_was_already_granted() {
        XCTAssertEqual(
            AccessibilityMonitorAction.resolve(wasGranted: true, isTrusted: true, hookRunning: false),
            .startHook
        )
    }

    func test_stable_permission_does_not_repeat_work() {
        XCTAssertEqual(
            AccessibilityMonitorAction.resolve(wasGranted: true, isTrusted: true, hookRunning: true),
            .none
        )
        XCTAssertEqual(
            AccessibilityMonitorAction.resolve(wasGranted: false, isTrusted: false, hookRunning: false),
            .none
        )
    }
}
