import XCTest
@testable import SlipkeyApp

final class HookKeyTests: XCTestCase {
    func testFromCharacterLowercasesAscii() {
        XCTAssertEqual(HookKey.from(character: "E"), HookKey.from(character: "e"))
    }

    func testNonAsciiBecomesOther() {
        XCTAssertEqual(HookKey.from(character: "中"), .other)
    }

    func testDigitsAreAlphaNum() {
        XCTAssertEqual(HookKey.from(character: "5"), .alphaNum("5"))
    }

    func testLeaderEqualsLeader() {
        XCTAssertEqual(HookKey.leader, HookKey.leader)
    }
}
