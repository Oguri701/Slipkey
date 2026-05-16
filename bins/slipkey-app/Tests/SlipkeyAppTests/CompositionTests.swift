import XCTest
@testable import SlipkeyApp

final class CompositionTests: XCTestCase {
    func testActiveCompositionDefers() {
        XCTAssertTrue(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: true,
            state: .active, possibleComposition: false, recentlyTyped: false))
    }

    func testInactiveCompositionDoesNotDefer() {
        XCTAssertFalse(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: true,
            state: .inactive, possibleComposition: true, recentlyTyped: true))
    }

    func testUnknownDefersIfPossibleOrRecent() {
        XCTAssertTrue(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: true,
            state: .unknown, possibleComposition: true, recentlyTyped: false))
        XCTAssertTrue(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: true,
            state: .unknown, possibleComposition: false, recentlyTyped: true))
        XCTAssertFalse(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: true,
            state: .unknown, possibleComposition: false, recentlyTyped: false))
    }

    func testLeaderOnlyDefersForConfirmedActiveComposition() {
        XCTAssertTrue(Composition.shouldDeferLeader(sourceIsInputMethod: true, state: .active))
        XCTAssertFalse(Composition.shouldDeferLeader(sourceIsInputMethod: true, state: .unknown))
        XCTAssertFalse(Composition.shouldDeferLeader(sourceIsInputMethod: true, state: .inactive))
        XCTAssertFalse(Composition.shouldDeferLeader(sourceIsInputMethod: false, state: .active))
    }

    func testNonIdleNeverDefers() {
        XCTAssertFalse(Composition.shouldDefer(
            idle: false, sourceIsInputMethod: true,
            state: .active, possibleComposition: true, recentlyTyped: true))
    }

    func testKeyboardLayoutNeverDefers() {
        XCTAssertFalse(Composition.shouldDefer(
            idle: true, sourceIsInputMethod: false,
            state: .active, possibleComposition: true, recentlyTyped: true))
    }
}
