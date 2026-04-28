import XCTest
@testable import SlipkeyApp

final class KeycodeTests: XCTestCase {
    func testDefaultLeaderIsSemicolon() {
        XCTAssertEqual(Keycode.toKey(Keycode.semicolon, leader: Keycode.semicolon), .leader)
        XCTAssertEqual(Keycode.fromKey(.leader, leader: Keycode.semicolon), Keycode.semicolon)
    }

    func testCustomLeaderRemapsKeycode() {
        let comma = Keycode.leaderKeycode(for: ",")!
        XCTAssertEqual(Keycode.toKey(comma, leader: comma), .leader)
        XCTAssertEqual(Keycode.toKey(Keycode.semicolon, leader: comma), .other)
        XCTAssertEqual(Keycode.fromKey(.leader, leader: comma), comma)
    }

    func testAlphaNumRoundTrips() {
        for kc: UInt16 in [0x00, 0x0E, 0x26, 0x06, 0x12, 0x19] {
            let key = Keycode.toKey(kc, leader: Keycode.semicolon)
            guard case .alphaNum = key else {
                return XCTFail("kc \(kc) did not map to alphaNum: got \(key)")
            }
            XCTAssertEqual(Keycode.fromKey(key, leader: Keycode.semicolon), kc)
        }
    }

    func testLeaderForUppercaseLetterReturnsKeycode() {
        // Uppercase letters need Shift on US-QWERTY but the API normalizes
        // input via lowercased() before lookup, mirroring the Rust impl.
        XCTAssertEqual(Keycode.leaderKeycode(for: "A"), 0x00)
        XCTAssertEqual(Keycode.leaderKeycode(for: "a"), 0x00)
    }
}
