import XCTest
@testable import SlipkeyApp

final class StateMachineTests: XCTestCase {
    private func k(_ c: Character) -> HookKey { HookKey.from(character: c) }

    private func feed(_ sm: inout StateMachine, _ keys: [HookKey]) -> StateMachineResponse {
        var last = StateMachineResponse.pass
        for key in keys { last = sm.onKeydown(key) }
        return last
    }

    func testIdlePassthrough() {
        var sm = StateMachine.defaults()
        let r = sm.onKeydown(k("e"))
        XCTAssertFalse(r.suppress)
        XCTAssertTrue(r.replay.isEmpty)
        XCTAssertNil(r.switchTo)
    }

    func testFullTriggerEn() {
        var sm = StateMachine.defaults()
        _ = sm.onKeydown(.leader)
        _ = sm.onKeydown(k("e"))
        let r = sm.onKeydown(k("n"))
        XCTAssertTrue(r.suppress)
        XCTAssertEqual(r.switchTo, "en")
        XCTAssertTrue(r.replay.isEmpty)
    }

    func testFullTriggerJa() {
        var sm = StateMachine.defaults()
        let r = feed(&sm, [.leader, k("j"), k("a")])
        XCTAssertEqual(r.switchTo, "ja")
        XCTAssertTrue(r.suppress)
    }

    func testFullTriggerZh() {
        var sm = StateMachine.defaults()
        let r = feed(&sm, [.leader, k("z"), k("h")])
        XCTAssertEqual(r.switchTo, "zh")
        XCTAssertTrue(r.suppress)
    }

    func testCustomLanguagePrefixes() {
        var sm = StateMachine(mappings: [(language: "fr", prefix: "fr"), (language: "ko", prefix: "ko")])
        XCTAssertEqual(feed(&sm, [.leader, k("f"), k("r")]).switchTo, "fr")
        XCTAssertEqual(feed(&sm, [.leader, k("k"), k("o")]).switchTo, "ko")
    }

    func testLongPrefixDoesNotTriggerEarly() {
        var sm = StateMachine(mappings: [(language: "en", prefix: "eng")])
        XCTAssertNil(feed(&sm, [.leader, k("e"), k("n")]).switchTo)
        XCTAssertFalse(sm.isIdle)
        let r = sm.onKeydown(k("g"))
        XCTAssertEqual(r.switchTo, "en")
    }

    func testCancelAtLeaderBranch() {
        var sm = StateMachine.defaults()
        _ = sm.onKeydown(.leader)
        let r = sm.onKeydown(.other)
        XCTAssertFalse(r.suppress)
        XCTAssertEqual(r.replay, [.leader])
        XCTAssertNil(r.switchTo)
    }

    func testCancelMidSequence() {
        var sm = StateMachine.defaults()
        _ = sm.onKeydown(.leader)
        _ = sm.onKeydown(k("e"))
        let r = sm.onKeydown(k("k"))
        XCTAssertFalse(r.suppress)
        XCTAssertEqual(r.replay, [.leader, k("e")])
    }

    func testWrongL2ForJCancels() {
        var sm = StateMachine.defaults()
        _ = sm.onKeydown(.leader)
        _ = sm.onKeydown(k("j"))
        let r = sm.onKeydown(k("z"))
        XCTAssertFalse(r.suppress)
        XCTAssertEqual(r.replay, [.leader, k("j")])
        XCTAssertTrue(sm.isIdle)
    }

    func testDoubleLeaderRestarts() {
        var sm = StateMachine.defaults()
        let r1 = sm.onKeydown(.leader)
        XCTAssertTrue(r1.suppress)
        let r2 = sm.onKeydown(.leader)
        XCTAssertTrue(r2.suppress)
        XCTAssertEqual(r2.replay, [.leader])
    }

    func testLeaderRestartsMidSequence() {
        var sm = StateMachine.defaults()
        let r = feed(&sm, [.leader, k("e"), .leader, k("j"), k("a")])
        XCTAssertEqual(r.switchTo, "ja")
    }

    func testResetClearsState() {
        var sm = StateMachine.defaults()
        _ = sm.onKeydown(.leader)
        _ = sm.onKeydown(k("e"))
        sm.reset()
        let r = sm.onKeydown(k("n"))
        XCTAssertFalse(r.suppress)
        XCTAssertNil(r.switchTo)
    }

    func testSequentialTriggers() {
        var sm = StateMachine.defaults()
        XCTAssertEqual(feed(&sm, [.leader, k("e"), k("n")]).switchTo, "en")
        XCTAssertEqual(feed(&sm, [.leader, k("j"), k("a")]).switchTo, "ja")
        XCTAssertEqual(feed(&sm, [.leader, k("z"), k("h")]).switchTo, "zh")
    }
}
