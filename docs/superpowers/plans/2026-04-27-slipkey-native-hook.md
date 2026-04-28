# Slipkey Native macOS Hook Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the IME-switching event hook from a Rust subprocess (`imeswitchd`) into the Slipkey app's main Swift process, so a single TCC (Accessibility) authorization covers everything and switching actually works after install. Mirror Mos's architecture: pure Swift, single binary, no daemon child process.

**Architecture:** All macOS-specific logic — virtual-keycode mapping, trigger state machine, CGEventTap install, composition heuristic, TIS-based IME switching — lives inside the Slipkey SwiftPM target. The Rust crates (`imeswitch-core`, `imeswitch-windows`, `bins/imeswitchd`) survive untouched; `imeswitchd` becomes a CLI-only diagnostic tool (`list`/`init`/`wizard`), no longer spawned by Slipkey. The `imeswitch-macos` crate is deleted.

**Tech Stack:** Swift 5.9, SwiftPM, AppKit, SwiftUI, CoreGraphics (CGEventTap), Carbon (TIS via Swift FFI), ApplicationServices (Accessibility), XCTest.

---

## File Structure

### New files (Slipkey)

| Path | Responsibility |
|---|---|
| `Sources/SlipkeyApp/Hook/HookKey.swift` | `HookKey` enum (Leader, alphaNum, other) |
| `Sources/SlipkeyApp/Hook/Keycode.swift` | US-QWERTY virtual keycode constants + conversions |
| `Sources/SlipkeyApp/Hook/StateMachine.swift` | Trie-based trigger state machine; pure value type |
| `Sources/SlipkeyApp/Hook/IMEManager.swift` | TIS Carbon FFI: list, switch, current-source-id, source-kind |
| `Sources/SlipkeyApp/Hook/Composition.swift` | AX-based composition detection + helper logic |
| `Sources/SlipkeyApp/Hook/EventHook.swift` | CGEventTap install + callback driving the state machine |
| `Sources/SlipkeyApp/Services/HookService.swift` | High-level: load mapping, install/restart hook, log switches |
| `Tests/SlipkeyAppTests/StateMachineTests.swift` | Port of `imeswitch-core` state-machine unit tests |
| `Tests/SlipkeyAppTests/KeycodeTests.swift` | Round-trip + leader-remap tests |
| `Tests/SlipkeyAppTests/CompositionTests.swift` | Pure-logic tests for `shouldDeferForComposition` |

### Files modified

| Path | Change |
|---|---|
| `bins/slipkey-app/Package.swift` | Add test target |
| `Sources/SlipkeyApp/App/AppDelegate.swift` | Start `HookService` instead of `DaemonService` |
| `Sources/SlipkeyApp/App/AppState.swift` | Replace `daemon` with `hook` (HookService) |
| `Sources/SlipkeyApp/Services/InputSourceService.swift` | Drop subprocess; call `IMEManager.listAll()` |
| `scripts/package-macos.sh` | Stop copying `imeswitchd` into `Slipkey.app/Contents/Resources/`; drop `--deep` codesign hack since there's no nested binary |

### Files deleted

| Path | Reason |
|---|---|
| `Sources/SlipkeyApp/Services/DaemonService.swift` | Replaced by HookService |

### Out of scope (untouched)

- All `crates/imeswitch-core` Rust code (Windows port reuses it)
- All `crates/imeswitch-windows` Rust code
- `bins/imeswitchd` (kept as diagnostic CLI; just not spawned)
- `crates/imeswitch-macos` will be **deleted** in Task 12 once everything works

---

## Task 0: Add XCTest target to Package.swift

**Files:**
- Modify: `bins/slipkey-app/Package.swift`
- Create: `bins/slipkey-app/Tests/SlipkeyAppTests/SmokeTests.swift`

- [ ] **Step 1: Replace `Package.swift` with the test-enabled version**

```swift
// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "SlipkeyApp",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "Slipkey", targets: ["SlipkeyApp"])
    ],
    targets: [
        .executableTarget(name: "SlipkeyApp"),
        .testTarget(
            name: "SlipkeyAppTests",
            dependencies: ["SlipkeyApp"]
        )
    ]
)
```

- [ ] **Step 2: Create a smoke test to verify the test target compiles**

```swift
// bins/slipkey-app/Tests/SlipkeyAppTests/SmokeTests.swift
import XCTest
@testable import SlipkeyApp

final class SmokeTests: XCTestCase {
    func testTwoPlusTwo() {
        XCTAssertEqual(2 + 2, 4)
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: `Test Suite 'All tests' passed`. If you get an `@testable import` error, that means SlipkeyApp is an executableTarget; @testable import works on executable targets in Swift 5.9+.

- [ ] **Step 4: Commit**

```bash
git add bins/slipkey-app/Package.swift bins/slipkey-app/Tests/SlipkeyAppTests/SmokeTests.swift
git commit -m "test: add XCTest target to slipkey-app"
```

---

## Task 1: HookKey enum

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/HookKey.swift`
- Create: `bins/slipkey-app/Tests/SlipkeyAppTests/HookKeyTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
// bins/slipkey-app/Tests/SlipkeyAppTests/HookKeyTests.swift
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
```

- [ ] **Step 2: Run test to verify it fails (compile error)**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter HookKeyTests`
Expected: compile error (`HookKey` undefined).

- [ ] **Step 3: Implement `HookKey`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/HookKey.swift
import Foundation

enum HookKey: Hashable {
    case leader
    case alphaNum(Character)
    case other

    static func from(character raw: Character) -> HookKey {
        guard raw.isASCII, raw.isLetter || raw.isNumber else { return .other }
        return .alphaNum(Character(raw.lowercased()))
    }
}
```

Note: Swift 6.3 does not allow a case and a static func to share the same name on the same enum. Callers must use `HookKey.from(character: c)` to construct from arbitrary input — never `HookKey.alphaNum(c)` directly with anything but already-lowercase ASCII. The case `.alphaNum` is exposed for pattern matching; downstream code that constructs from arbitrary characters must go through `from(character:)` so the stored value is always normalized.

- [ ] **Step 4: Run tests to verify they pass**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter HookKeyTests`
Expected: 4 passing tests.

- [ ] **Step 5: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/HookKey.swift bins/slipkey-app/Tests/SlipkeyAppTests/HookKeyTests.swift
git commit -m "feat(hook): add HookKey enum with ASCII normalization"
```

---

## Task 2: US-QWERTY keycode mapping

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/Keycode.swift`
- Create: `bins/slipkey-app/Tests/SlipkeyAppTests/KeycodeTests.swift`

- [ ] **Step 1: Write the failing tests**

```swift
// bins/slipkey-app/Tests/SlipkeyAppTests/KeycodeTests.swift
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

    func testLeaderForUppercaseLetterReturnsNil() {
        XCTAssertNil(Keycode.leaderKeycode(for: "A"))
        // Uppercase letters need Shift on US-QWERTY → would clash with modifier guard.
        // The Rust impl lowercases first; we mirror that — lowercase 'a' is fine.
        XCTAssertEqual(Keycode.leaderKeycode(for: "a"), 0x00)
    }
}
```

- [ ] **Step 2: Run tests to confirm fail**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter KeycodeTests`
Expected: compile error.

- [ ] **Step 3: Implement `Keycode`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/Keycode.swift
import Foundation

enum Keycode {
    static let semicolon: UInt16 = 0x29

    /// Maps a leader character (`;`, `,`, `/`, …) to the US-QWERTY keycode
    /// that produces it without modifiers. Returns nil for chars that need
    /// Shift on US-QWERTY (those would collide with the Shift modifier guard).
    static func leaderKeycode(for char: Character) -> UInt16? {
        switch char {
        case ";": return 0x29
        case ",": return 0x2B
        case ".": return 0x2F
        case "/": return 0x2C
        case "'": return 0x27
        case "\\": return 0x2A
        case "[": return 0x21
        case "]": return 0x1E
        case "-": return 0x1B
        case "=": return 0x18
        case "`": return 0x32
        default:
            guard char.isASCII, char.isLetter || char.isNumber else { return nil }
            return alphaNumKeycode(Character(char.lowercased()))
        }
    }

    static func toKey(_ kc: UInt16, leader: UInt16) -> HookKey {
        if kc == leader { return .leader }
        guard let ch = alphaNumChar(for: kc) else { return .other }
        return HookKey.alphaNum(ch)
    }

    static func fromKey(_ key: HookKey, leader: UInt16) -> UInt16? {
        switch key {
        case .leader: return leader
        case .alphaNum(let c): return alphaNumKeycode(c)
        case .other: return nil
        }
    }

    private static func alphaNumChar(for kc: UInt16) -> Character? {
        switch kc {
        case 0x00: return "a"
        case 0x0B: return "b"
        case 0x08: return "c"
        case 0x02: return "d"
        case 0x0E: return "e"
        case 0x03: return "f"
        case 0x05: return "g"
        case 0x04: return "h"
        case 0x22: return "i"
        case 0x26: return "j"
        case 0x28: return "k"
        case 0x25: return "l"
        case 0x2E: return "m"
        case 0x2D: return "n"
        case 0x1F: return "o"
        case 0x23: return "p"
        case 0x0C: return "q"
        case 0x0F: return "r"
        case 0x01: return "s"
        case 0x11: return "t"
        case 0x20: return "u"
        case 0x09: return "v"
        case 0x0D: return "w"
        case 0x07: return "x"
        case 0x10: return "y"
        case 0x06: return "z"
        case 0x1D: return "0"
        case 0x12: return "1"
        case 0x13: return "2"
        case 0x14: return "3"
        case 0x15: return "4"
        case 0x17: return "5"
        case 0x16: return "6"
        case 0x1A: return "7"
        case 0x1C: return "8"
        case 0x19: return "9"
        default: return nil
        }
    }

    private static func alphaNumKeycode(_ c: Character) -> UInt16? {
        switch c {
        case "a": return 0x00
        case "b": return 0x0B
        case "c": return 0x08
        case "d": return 0x02
        case "e": return 0x0E
        case "f": return 0x03
        case "g": return 0x05
        case "h": return 0x04
        case "i": return 0x22
        case "j": return 0x26
        case "k": return 0x28
        case "l": return 0x25
        case "m": return 0x2E
        case "n": return 0x2D
        case "o": return 0x1F
        case "p": return 0x23
        case "q": return 0x0C
        case "r": return 0x0F
        case "s": return 0x01
        case "t": return 0x11
        case "u": return 0x20
        case "v": return 0x09
        case "w": return 0x0D
        case "x": return 0x07
        case "y": return 0x10
        case "z": return 0x06
        case "0": return 0x1D
        case "1": return 0x12
        case "2": return 0x13
        case "3": return 0x14
        case "4": return 0x15
        case "5": return 0x17
        case "6": return 0x16
        case "7": return 0x1A
        case "8": return 0x1C
        case "9": return 0x19
        default: return nil
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter KeycodeTests`
Expected: 4 passing tests.

- [ ] **Step 5: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/Keycode.swift bins/slipkey-app/Tests/SlipkeyAppTests/KeycodeTests.swift
git commit -m "feat(hook): add US-QWERTY keycode mapping"
```

---

## Task 3: Trigger state machine

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/StateMachine.swift`
- Create: `bins/slipkey-app/Tests/SlipkeyAppTests/StateMachineTests.swift`

- [ ] **Step 1: Write all the failing tests (port of `imeswitch-core` test suite)**

```swift
// bins/slipkey-app/Tests/SlipkeyAppTests/StateMachineTests.swift
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
```

- [ ] **Step 2: Run tests to confirm they fail (compile error)**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter StateMachineTests`
Expected: compile error (`StateMachine`/`StateMachineResponse` undefined).

- [ ] **Step 3: Implement `StateMachine` and `StateMachineResponse`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/StateMachine.swift
import Foundation

struct StateMachineResponse: Equatable {
    /// If true, the hook must NOT forward the current key to the OS.
    var suppress: Bool
    /// Previously-suppressed keys to synth-post back into the event stream
    /// (in order) BEFORE the current key is handled.
    var replay: [HookKey]
    /// If set, the hook should switch the IME to this language after
    /// handling this key. Lowercase ISO-639-1 (e.g. "en", "ja", "zh").
    var switchTo: String?

    static let pass = StateMachineResponse(suppress: false, replay: [], switchTo: nil)
    static let suppressed = StateMachineResponse(suppress: true, replay: [], switchTo: nil)

    static func switchTo(_ language: String) -> StateMachineResponse {
        StateMachineResponse(suppress: true, replay: [], switchTo: language)
    }

    static func cancel(replay: [HookKey]) -> StateMachineResponse {
        StateMachineResponse(suppress: false, replay: replay, switchTo: nil)
    }
}

struct StateMachine {
    private struct TrieNode {
        var children: [HookKey: Int] = [:]
        var terminal: String?
    }

    private var nodes: [TrieNode] = [TrieNode()]
    private var current: Int = 0
    private var buffer: [HookKey] = []

    static func defaults() -> StateMachine {
        StateMachine(mappings: [
            (language: "en", prefix: "en"),
            (language: "ja", prefix: "ja"),
            (language: "zh", prefix: "zh")
        ])
    }

    init(mappings: [(language: String, prefix: String)]) {
        for (language, prefix) in mappings {
            insert(language: language.lowercased(), prefix: prefix)
        }
    }

    var isIdle: Bool { current == 0 }

    mutating func reset() {
        current = 0
        buffer.removeAll(keepingCapacity: true)
    }

    mutating func onKeydown(_ key: HookKey) -> StateMachineResponse {
        if isIdle {
            return startOrPass(key)
        }
        if let next = nodes[current].children[key] {
            return advance(to: next, with: key)
        }

        let replay = buffer
        buffer.removeAll(keepingCapacity: true)
        current = 0

        if let next = nodes[0].children[key] {
            buffer.append(key)
            current = next
            let lang = finishIfTerminal()
            return StateMachineResponse(suppress: true, replay: replay, switchTo: lang)
        }
        return .cancel(replay: replay)
    }

    private mutating func startOrPass(_ key: HookKey) -> StateMachineResponse {
        if let next = nodes[0].children[key] {
            return advance(to: next, with: key)
        }
        return .pass
    }

    private mutating func advance(to next: Int, with key: HookKey) -> StateMachineResponse {
        buffer.append(key)
        current = next
        if let lang = finishIfTerminal() {
            return .switchTo(lang)
        }
        return .suppressed
    }

    private mutating func finishIfTerminal() -> String? {
        guard let lang = nodes[current].terminal else { return nil }
        reset()
        return lang
    }

    private mutating func insert(language: String, prefix: String) {
        guard !prefix.isEmpty else { return }
        var node = 0
        let keys: [HookKey] = [.leader] + prefix.map { HookKey.from(character: $0) }
        for key in keys {
            if case .other = key { return }
            if let next = nodes[node].children[key] {
                node = next
            } else {
                let next = nodes.count
                nodes.append(TrieNode())
                nodes[node].children[key] = next
                node = next
            }
        }
        nodes[node].terminal = language
    }
}
```

- [ ] **Step 4: Run tests**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter StateMachineTests`
Expected: all 13 tests pass.

- [ ] **Step 5: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/StateMachine.swift bins/slipkey-app/Tests/SlipkeyAppTests/StateMachineTests.swift
git commit -m "feat(hook): add trie-based trigger state machine"
```

---

## Task 4: IME manager (TIS wrapper)

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/IMEManager.swift`

No unit tests — TIS is global system state, not unit-testable. Functional verification happens in Task 11.

- [ ] **Step 1: Implement `IMEManager`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/IMEManager.swift
import Carbon
import Foundation

enum CurrentSourceKind {
    case keyboardLayout
    case inputMethod
    case other
}

struct TISSourceInfo {
    let id: String
    let name: String
    let category: String
    let type: String
    let languages: [String]
    let isEnabled: Bool
    let isSelectable: Bool
}

enum IMESwitchError: Error, CustomStringConvertible {
    case notInstalled(String)
    case notSelectable(String)
    case selectFailed(id: String, status: OSStatus)

    var description: String {
        switch self {
        case .notInstalled(let id):
            return "input source '\(id)' not installed — enable it in System Settings → Keyboard → Input Sources"
        case .notSelectable(let id):
            return "input source '\(id)' is installed but not selectable"
        case .selectFailed(let id, let status):
            return "TISSelectInputSource('\(id)') failed with OSStatus \(status)"
        }
    }
}

enum IMEManager {
    /// Synchronously selects the input source by its ID (e.g. "com.apple.keylayout.ABC").
    static func select(sourceID: String) -> Result<Void, IMESwitchError> {
        let filter = [kTISPropertyInputSourceID!: sourceID] as CFDictionary
        guard let listRef = TISCreateInputSourceList(filter, false) else {
            return .failure(.notInstalled(sourceID))
        }
        let list = listRef.takeRetainedValue() as NSArray
        guard let first = list.firstObject else {
            return .failure(.notInstalled(sourceID))
        }
        let source = first as! TISInputSource
        guard readBool(source, key: kTISPropertyInputSourceIsSelectCapable!) == true else {
            return .failure(.notSelectable(sourceID))
        }
        let status = TISSelectInputSource(source)
        if status != noErr {
            return .failure(.selectFailed(id: sourceID, status: status))
        }
        return .success(())
    }

    static func currentSourceID() -> String? {
        guard let srcRef = TISCopyCurrentKeyboardInputSource() else { return nil }
        let src = srcRef.takeRetainedValue()
        return readString(src, key: kTISPropertyInputSourceID!)
    }

    static func currentSourceKind() -> CurrentSourceKind {
        guard let srcRef = TISCopyCurrentKeyboardInputSource() else { return .other }
        let src = srcRef.takeRetainedValue()
        switch readString(src, key: kTISPropertyInputSourceType!) {
        case "TISTypeKeyboardLayout":
            return .keyboardLayout
        case "TISTypeKeyboardInputMode",
             "TISTypeKeyboardInputMethodWithoutModes",
             "TISTypeKeyboardInputMethodModeEnabled":
            return .inputMethod
        default:
            return .other
        }
    }

    /// Lists every input source the system reports (enabled or not).
    /// Used by the SettingsView to populate the IME picker.
    static func listAll() -> [TISSourceInfo] {
        guard let listRef = TISCreateInputSourceList(nil, true) else { return [] }
        let list = listRef.takeRetainedValue() as NSArray
        return list.compactMap { item -> TISSourceInfo? in
            let src = item as! TISInputSource
            return TISSourceInfo(
                id: readString(src, key: kTISPropertyInputSourceID!) ?? "",
                name: readString(src, key: kTISPropertyLocalizedName!) ?? "",
                category: readString(src, key: kTISPropertyInputSourceCategory!) ?? "",
                type: readString(src, key: kTISPropertyInputSourceType!) ?? "",
                languages: readStringArray(src, key: kTISPropertyInputSourceLanguages!) ?? [],
                isEnabled: readBool(src, key: kTISPropertyInputSourceIsEnabled!) ?? false,
                isSelectable: readBool(src, key: kTISPropertyInputSourceIsSelectCapable!) ?? false
            )
        }
    }

    // MARK: - Private CF readers

    private static func readString(_ source: TISInputSource, key: CFString) -> String? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        return Unmanaged<CFString>.fromOpaque(ptr).takeUnretainedValue() as String
    }

    private static func readBool(_ source: TISInputSource, key: CFString) -> Bool? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        return CFBooleanGetValue(Unmanaged<CFBoolean>.fromOpaque(ptr).takeUnretainedValue())
    }

    private static func readStringArray(_ source: TISInputSource, key: CFString) -> [String]? {
        guard let ptr = TISGetInputSourceProperty(source, key) else { return nil }
        let array = Unmanaged<CFArray>.fromOpaque(ptr).takeUnretainedValue() as NSArray
        return array.compactMap { $0 as? String }
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `swift build --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: Build succeeded. No warnings about Carbon — Swift can import Carbon directly.

- [ ] **Step 3: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/IMEManager.swift
git commit -m "feat(hook): add Carbon TIS wrapper for IME switching"
```

---

## Task 5: Composition detection

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/Composition.swift`
- Create: `bins/slipkey-app/Tests/SlipkeyAppTests/CompositionTests.swift`

This task ports the AX-based composition detection. The candidate-window scan in the Rust code (`ime_candidate_window_is_visible`) is **deferred** — AX catches the common case; we'll add the candidate-window fallback later only if real users hit issues.

- [ ] **Step 1: Write tests for the pure-logic helper**

```swift
// bins/slipkey-app/Tests/SlipkeyAppTests/CompositionTests.swift
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
```

- [ ] **Step 2: Run tests to confirm fail**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter CompositionTests`
Expected: compile error.

- [ ] **Step 3: Implement `Composition`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/Composition.swift
import ApplicationServices
import Foundation

enum CompositionState {
    case active
    case inactive
    case unknown
}

enum Composition {
    /// 500ms after the last keystroke an IME is assumed to still hold a
    /// composition buffer. Matches `COMPOSITION_IDLE_THRESHOLD` in Rust.
    static let idleThreshold: TimeInterval = 0.5

    /// Pure-logic helper. The hook callback assembles its inputs and calls this.
    static func shouldDefer(
        idle: Bool,
        sourceIsInputMethod: Bool,
        state: CompositionState,
        possibleComposition: Bool,
        recentlyTyped: Bool
    ) -> Bool {
        guard idle, sourceIsInputMethod else { return false }
        switch state {
        case .active: return true
        case .inactive: return false
        case .unknown: return possibleComposition || recentlyTyped
        }
    }

    /// Asks Accessibility about the focused element's marked-text range.
    /// Returns `.unknown` if AX cannot answer (web views, opaque controls).
    static func focusedState() -> CompositionState {
        let system = AXUIElementCreateSystemWide()
        guard let focused = copyAttribute(system, attribute: "AXFocusedUIElement") else {
            return .unknown
        }
        let element = focused as! AXUIElement
        if let known = markedRangeState(element, attribute: "AXMarkedTextRange") {
            return known
        }
        if copyAttribute(element, attribute: "AXTextInputMarkedTextMarkerRange") != nil {
            return .active
        }
        return .unknown
    }

    /// Returns whether a keycode is a "composition-ending" key (Enter/Esc/Tab/Space).
    /// Matches Rust's `is_composition_ending_key`.
    static func isCompositionEnding(keycode: UInt16) -> Bool {
        switch keycode {
        case 0x24, 0x31, 0x33, 0x35: return true // return, space, delete, escape
        default: return false
        }
    }

    private static func copyAttribute(_ element: AXUIElement, attribute: String) -> CFTypeRef? {
        var value: CFTypeRef?
        let err = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
        guard err == .success, let v = value else { return nil }
        return v
    }

    /// If the attribute returns an AXValue/CFRange, length>0 = active, ==0 = inactive.
    /// Anything else → unknown so the caller can fall back.
    private static func markedRangeState(_ element: AXUIElement, attribute: String) -> CompositionState? {
        guard let raw = copyAttribute(element, attribute: attribute) else { return nil }
        guard CFGetTypeID(raw) == AXValueGetTypeID() else { return .unknown }
        let axValue = raw as! AXValue
        guard AXValueGetType(axValue) == .cfRange else { return .unknown }
        var range = CFRange(location: 0, length: 0)
        guard AXValueGetValue(axValue, .cfRange, &range) else { return .unknown }
        return range.length > 0 ? .active : .inactive
    }
}
```

- [ ] **Step 4: Run tests**

Run: `swift test --package-path bins/slipkey-app --scratch-path target/slipkey-swift --filter CompositionTests`
Expected: 5 passing tests.

- [ ] **Step 5: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/Composition.swift bins/slipkey-app/Tests/SlipkeyAppTests/CompositionTests.swift
git commit -m "feat(hook): AX-based composition detection"
```

---

## Task 6: Event hook (CGEventTap install + callback)

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Hook/EventHook.swift`

No unit tests — CGEventTap requires the OS event stream and Accessibility permission. Functional verification in Task 11.

- [ ] **Step 1: Implement `EventHook`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Hook/EventHook.swift
import AppKit
import CoreGraphics
import Foundation

enum EventHookError: Error, CustomStringConvertible {
    case tapCreationFailed
    case runLoopSourceFailed

    var description: String {
        switch self {
        case .tapCreationFailed:
            return "CGEvent.tapCreate returned nil — Accessibility permission not granted to Slipkey"
        case .runLoopSourceFailed:
            return "CFMachPortCreateRunLoopSource returned nil"
        }
    }
}

/// Installs a HID-level keydown tap on the current thread's CFRunLoop and
/// drives the trigger state machine for every keydown.
///
/// Lifetime: keep the EventHook instance alive for as long as the hook is
/// installed. `deinit` removes the run-loop source and invalidates the port.
///
/// Threading: every call site (install, uninstall, the C callback) is on the
/// main thread because the tap's run-loop source is added to the main run loop.
/// We don't mark this class `@MainActor` because the C callback is a static
/// `@convention(c)` function with no ambient actor; isolating the class would
/// force every method call through awkward `MainActor.assumeIsolated` blocks.
final class EventHook {
    private let stateMachine: Box<StateMachine>
    private let leaderKeycode: UInt16
    private let onSwitch: (String) -> Void
    private let onLog: (String) -> Void

    private var lastKeydown: Date?
    private var possibleComposition: Bool = false

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?

    init(
        leaderKeycode: UInt16,
        mappings: [(language: String, prefix: String)],
        onSwitch: @escaping (String) -> Void,
        onLog: @escaping (String) -> Void = { _ in }
    ) {
        self.stateMachine = Box(StateMachine(mappings: mappings))
        self.leaderKeycode = leaderKeycode
        self.onSwitch = onSwitch
        self.onLog = onLog
    }

    func install() throws {
        let mask: CGEventMask = 1 << CGEventType.keyDown.rawValue
        let info = Unmanaged.passUnretained(self).toOpaque()

        guard let tap = CGEvent.tapCreate(
            tap: .cghidEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: Self.tapCallback,
            userInfo: info
        ) else {
            throw EventHookError.tapCreationFailed
        }
        guard let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0) else {
            CFMachPortInvalidate(tap)
            throw EventHookError.runLoopSourceFailed
        }
        CFRunLoopAddSource(CFRunLoopGetCurrent(), src, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        self.tap = tap
        self.runLoopSource = src
    }

    func uninstall() {
        if let src = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), src, .commonModes)
            self.runLoopSource = nil
        }
        if let tap = tap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
            self.tap = nil
        }
    }

    deinit {
        if let src = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), src, .commonModes)
        }
        if let tap = tap {
            CFMachPortInvalidate(tap)
        }
    }

    // MARK: - C-style callback

    private static let tapCallback: CGEventTapCallBack = { _, type, event, info in
        guard let info = info else { return Unmanaged.passUnretained(event) }
        let hook = Unmanaged<EventHook>.fromOpaque(info).takeUnretainedValue()

        // Re-enable the tap if the system disabled it (timeout / user input switch).
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let tap = hook.tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return Unmanaged.passUnretained(event)
        }

        guard type == .keyDown else { return Unmanaged.passUnretained(event) }
        return hook.handleKeyDown(event)
    }

    private func handleKeyDown(_ event: CGEvent) -> Unmanaged<CGEvent>? {
        let keycode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        let flags = event.flags
        let now = Date()

        let idle = stateMachine.value.isIdle
        let recentlyTyped: Bool
        if let last = lastKeydown {
            recentlyTyped = now.timeIntervalSince(last) < Composition.idleThreshold
        } else {
            recentlyTyped = false
        }
        let sourceKind = IMEManager.currentSourceKind()
        let compositionState: CompositionState
        if sourceKind == .inputMethod {
            compositionState = Composition.focusedState()
        } else {
            compositionState = .inactive
        }
        let shouldDefer = Composition.shouldDefer(
            idle: idle,
            sourceIsInputMethod: sourceKind == .inputMethod,
            state: compositionState,
            possibleComposition: possibleComposition,
            recentlyTyped: recentlyTyped
        )

        if shouldDefer {
            lastKeydown = now
            updatePossibleComposition(sourceKind: sourceKind, state: compositionState, keycode: keycode, didSwitch: false)
            return Unmanaged.passUnretained(event)
        }

        let key = Self.eventKey(keycode: keycode, flags: flags, leader: leaderKeycode)
        let response = stateMachine.value.onKeydown(key)
        lastKeydown = now
        updatePossibleComposition(
            sourceKind: sourceKind,
            state: compositionState,
            keycode: keycode,
            didSwitch: response.switchTo != nil
        )

        if let lang = response.switchTo {
            onSwitch(lang)
        }

        for k in response.replay {
            if let kc = Keycode.fromKey(k, leader: leaderKeycode) {
                Self.synthPost(keycode: kc)
            }
        }

        if response.suppress {
            return nil  // drop the event
        }
        return Unmanaged.passUnretained(event)
    }

    private func updatePossibleComposition(
        sourceKind: CurrentSourceKind, state: CompositionState, keycode: UInt16, didSwitch: Bool
    ) {
        if didSwitch || sourceKind != .inputMethod {
            possibleComposition = false
            return
        }
        switch state {
        case .active:
            possibleComposition = true
        case .inactive:
            possibleComposition = false
        case .unknown:
            if Composition.isCompositionEnding(keycode: keycode) {
                possibleComposition = false
            } else {
                possibleComposition = true
            }
        }
    }

    private static func eventKey(keycode: UInt16, flags: CGEventFlags, leader: UInt16) -> HookKey {
        let blocking: CGEventFlags = [.maskShift, .maskControl, .maskAlternate, .maskCommand]
        if !flags.intersection(blocking).isEmpty {
            return .other
        }
        return Keycode.toKey(keycode, leader: leader)
    }

    /// Synth-post the given keycode at session level (not HID) so it doesn't
    /// re-enter our own tap.
    private static func synthPost(keycode: UInt16) {
        guard let src = CGEventSource(stateID: .hidSystemState) else { return }
        if let down = CGEvent(keyboardEventSource: src, virtualKey: keycode, keyDown: true) {
            down.post(tap: .cgSessionEventTap)
        }
        if let up = CGEvent(keyboardEventSource: src, virtualKey: keycode, keyDown: false) {
            up.post(tap: .cgSessionEventTap)
        }
    }
}

/// Boxing wrapper so the hook's mutable state machine (a value type) can live
/// on the heap behind a stable identity. The C callback recovers `self` via
/// the opaque pointer; from there we mutate `box.value` in place.
private final class Box<T> {
    var value: T
    init(_ value: T) { self.value = value }
}
```

- [ ] **Step 2: Build to verify**

Run: `swift build --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: Build succeeded.

If you see Swift Concurrency warnings about Sendable or actor isolation when called from a `@MainActor` context (HookService), they're warnings rather than errors in Swift 5.9 default settings. If they become errors under stricter settings later, mark the affected methods `nonisolated` rather than re-introducing `@MainActor` on EventHook.

- [ ] **Step 3: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Hook/EventHook.swift
git commit -m "feat(hook): CGEventTap event hook with replay + composition guard"
```

---

## Task 7: HookService (replaces DaemonService)

**Files:**
- Create: `bins/slipkey-app/Sources/SlipkeyApp/Services/HookService.swift`

- [ ] **Step 1: Implement `HookService`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Services/HookService.swift
import AppKit
import Foundation

@MainActor
final class HookService {
    private var hook: EventHook?

    /// Installs the hook using the given config. Replaces any existing hook.
    /// Returns true on success, false if `EventHook.install()` failed (e.g.
    /// no Accessibility permission).
    @discardableResult
    func start(with config: SlipkeyConfig) -> Bool {
        stop()
        let leaderChar = config.leader.first ?? ";"
        let leaderKC = Keycode.leaderKeycode(for: leaderChar) ?? Keycode.semicolon
        let mappings: [(language: String, prefix: String)] = config.mappings
            .filter { !$0.prefix.isEmpty }
            .map { (language: $0.language.lowercased(), prefix: $0.prefix) }

        let hook = EventHook(
            leaderKeycode: leaderKC,
            mappings: mappings,
            onSwitch: { [weak self] lang in
                self?.handleSwitch(lang: lang, config: config)
            },
            onLog: { msg in NSLog("Slipkey hook: %@", msg) }
        )
        do {
            try hook.install()
            self.hook = hook
            NSLog("Slipkey: hook installed (leader=%@, mappings=%d)", String(leaderChar), mappings.count)
            return true
        } catch {
            NSLog("Slipkey: hook install failed: %@", String(describing: error))
            return false
        }
    }

    func stop() {
        hook?.uninstall()
        hook = nil
    }

    func restart(with config: SlipkeyConfig) {
        _ = start(with: config)
    }

    var isRunning: Bool { hook != nil }

    private func handleSwitch(lang: String, config: SlipkeyConfig) {
        guard let entry = config.mappings.first(where: { $0.language.lowercased() == lang.lowercased() }) else {
            NSLog("Slipkey: switch %@ — no mapping configured", lang)
            return
        }
        let before = IMEManager.currentSourceID() ?? "<none>"
        let result = IMEManager.select(sourceID: entry.source)
        let after = IMEManager.currentSourceID() ?? "<none>"
        switch result {
        case .success:
            NSLog("Slipkey: switch %@: %@ -> %@", lang, before, after)
        case .failure(let err):
            NSLog("Slipkey: switch %@ failed: %@ (was: %@)", lang, String(describing: err), before)
        }
    }
}
```

- [ ] **Step 2: Build**

Run: `swift build --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: Build succeeded.

- [ ] **Step 3: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Services/HookService.swift
git commit -m "feat(hook): add HookService controller"
```

---

## Task 8: Wire HookService into AppState/AppDelegate

**Files:**
- Modify: `bins/slipkey-app/Sources/SlipkeyApp/App/AppState.swift`
- Modify: `bins/slipkey-app/Sources/SlipkeyApp/App/AppDelegate.swift`
- Delete: `bins/slipkey-app/Sources/SlipkeyApp/Services/DaemonService.swift`

- [ ] **Step 1: Update `AppState.swift`**

Open `bins/slipkey-app/Sources/SlipkeyApp/App/AppState.swift` and:

a) Replace the line `let daemon = DaemonService()` with:

```swift
    let hook = HookService()
```

b) Replace the `saveAndRestart()` method body so it calls `hook.restart(with:)` instead of `daemon.restart()`. The full updated method:

```swift
    func saveAndRestart() {
        do {
            try ConfigStore.save(config)
            hook.restart(with: config)
            statusMessage = L10n.text("Saved. Shortcuts are active now.", uiLanguage)
        } catch {
            statusMessage = error.localizedDescription
        }
    }
```

- [ ] **Step 2: Update `AppDelegate.swift`**

Replace the `applicationDidFinishLaunching` and `applicationWillTerminate` bodies:

```swift
    func applicationDidFinishLaunching(_ notification: Notification) {
        let statusItemManager = self.statusItemManager
        appState.menuBarIconVisibilityDidChange = { [weak statusItemManager] _ in
            statusItemManager?.applyVisibility()
        }
        appState.load()
        statusItemManager.applyVisibility()
        _ = appState.hook.start(with: appState.config)

        if !AccessibilityService.isTrusted {
            windowManager.showSettings()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        appState.hook.stop()
    }
```

- [ ] **Step 3: Delete `DaemonService.swift`**

```bash
rm bins/slipkey-app/Sources/SlipkeyApp/Services/DaemonService.swift
```

- [ ] **Step 4: Build to verify nothing else references `daemon`**

Run: `swift build --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: Build succeeded. If you see errors about `appState.daemon`, search the codebase and replace remaining call sites with `appState.hook`.

```bash
grep -rn "appState.daemon\|\.daemon\." bins/slipkey-app/Sources/
```
Expected: empty output.

- [ ] **Step 5: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/App/AppState.swift bins/slipkey-app/Sources/SlipkeyApp/App/AppDelegate.swift bins/slipkey-app/Sources/SlipkeyApp/Services/DaemonService.swift
git commit -m "refactor: replace DaemonService with native HookService"
```

---

## Task 9: Replace `InputSourceService` with `IMEManager.listAll()`

**Files:**
- Modify: `bins/slipkey-app/Sources/SlipkeyApp/Services/InputSourceService.swift`

The current `InputSourceService` spawns `imeswitchd list` as a subprocess. After this refactor, the bundle no longer ships `imeswitchd`, so this would break. Rewrite using `IMEManager.listAll()`.

- [ ] **Step 1: Rewrite `InputSourceService.swift`**

```swift
// bins/slipkey-app/Sources/SlipkeyApp/Services/InputSourceService.swift
import Foundation

struct InputSourceService {
    func discover() -> [InputSource] {
        var seen = Set<String>()
        var result: [InputSource] = []
        for src in IMEManager.listAll() {
            guard isRealTypingSource(src.type),
                  src.category == "TISCategoryKeyboardInputSource",
                  src.isEnabled,
                  src.isSelectable
            else { continue }
            guard let language = src.languages.compactMap(normalizedSupportedLanguage).first
            else { continue }
            let dedupeKey = "\(language)\t\(src.name)"
            guard seen.insert(dedupeKey).inserted else { continue }
            result.append(InputSource(
                language: language,
                sourceID: src.id,
                name: src.name,
                isSelectable: src.isSelectable
            ))
        }
        return result
    }

    private func isRealTypingSource(_ type: String) -> Bool {
        type == "TISTypeKeyboardLayout" || type == "TISTypeKeyboardInputMode"
    }

    private func normalizedSupportedLanguage(_ rawLanguage: String) -> String? {
        let language = rawLanguage.lowercased()
        if language == "en" || language.hasPrefix("en-") || language.hasPrefix("en_") { return "en" }
        if language == "ja" || language.hasPrefix("ja-") || language.hasPrefix("ja_") { return "ja" }
        if language == "zh" || language.hasPrefix("zh-") || language.hasPrefix("zh_") { return "zh" }
        return nil
    }
}
```

- [ ] **Step 2: Build**

Run: `swift build --package-path bins/slipkey-app --scratch-path target/slipkey-swift`
Expected: Build succeeded.

- [ ] **Step 3: Commit**

```bash
git add bins/slipkey-app/Sources/SlipkeyApp/Services/InputSourceService.swift
git commit -m "refactor: InputSourceService uses native IMEManager"
```

---

## Task 10: Update package script — drop imeswitchd from bundle

**Files:**
- Modify: `scripts/package-macos.sh`

- [ ] **Step 1: Remove daemon-related steps**

Open `scripts/package-macos.sh` and:

a) Delete the line that builds the daemon (currently around line 21):

```bash
echo "==> Building daemon"
cargo build --release -p imeswitchd
```

Replace with a comment explaining the daemon is now CLI-only:

```bash
# Note: imeswitchd is no longer bundled. It survives only as a standalone
# CLI for diagnostics — `cargo build --release -p imeswitchd` if you need it.
```

b) Delete the line that copies the daemon into the bundle (currently around line 37):

```bash
cp "$ROOT/target/release/imeswitchd" "$APP_PATH/Contents/Resources/imeswitchd"
```

c) Delete the chmod for the daemon (currently around line 40), simplify to:

```bash
chmod +x "$APP_PATH/Contents/MacOS/$APP_NAME"
```

d) Replace `codesign --force --deep --sign -` with the non-deep version, since there's no longer a nested binary in `Resources/`:

```bash
codesign --force --sign - "$APP_PATH"
```

- [ ] **Step 2: Run the package script end-to-end**

Run: `bash scripts/package-macos.sh`
Expected: prints `macOS bundle output:` with paths to `Slipkey.app` and the `.zip`. No errors.

- [ ] **Step 3: Verify the bundle has no imeswitchd**

```bash
ls /Users/zlb/Desktop/imeswitch/target/release/bundle/macos/Slipkey.app/Contents/Resources/
codesign --verify --deep --strict --verbose=2 /Users/zlb/Desktop/imeswitch/target/release/bundle/macos/Slipkey.app
```
Expected: only `icon.icns` in Resources (no imeswitchd). Codesign verify reports `valid on disk` and `satisfies its Designated Requirement`.

- [ ] **Step 4: Commit**

```bash
git add scripts/package-macos.sh
git commit -m "build: stop bundling imeswitchd into Slipkey.app"
```

---

## Task 11: End-to-end smoke verification

**Files:** none (manual verification, no code changes)

- [ ] **Step 1: Reset Slipkey's TCC entry so we test from scratch**

This is destructive but reversible (you'll just have to re-grant). Run:

```bash
tccutil reset Accessibility dev.zlb.imeswitch
```

- [ ] **Step 2: Kill any running instance and launch the new bundle**

```bash
pkill -x Slipkey 2>/dev/null
pkill -x imeswitchd 2>/dev/null
open /Users/zlb/Desktop/imeswitch/target/release/bundle/macos/Slipkey.app
```

- [ ] **Step 3: Grant Accessibility**

The Settings window should auto-open (because `AccessibilityService.isTrusted` is false). Click the "Grant Accessibility" button or open System Settings → Privacy & Security → Accessibility manually and toggle Slipkey on. macOS will require a Slipkey relaunch — re-launch via `open …Slipkey.app`.

- [ ] **Step 4: Verify hook log**

In a new Terminal window:

```bash
log stream --predicate 'process == "Slipkey"' --info
```

Expected: a line `Slipkey: hook installed (leader=;, mappings=N)` shortly after launch.

- [ ] **Step 5: Test the triggers**

Open any text field (Notes, TextEdit, the address bar). Type:
- `;en` → should switch to ABC layout, no `;en` typed
- `;ja` → should switch to Kotoeri Romaji, no `;ja` typed
- `;zh` → should switch to SCIM Shuangpin, no `;zh` typed
- `;ex` → should type `;ex` literally (cancel-and-replay)

Each successful switch should log a line like `Slipkey: switch en: com.apple.inputmethod.SCIM.Shuangpin -> com.apple.keylayout.ABC` in the `log stream` window.

- [ ] **Step 6: Verify no orphan imeswitchd**

```bash
ps aux | grep -E "(imeswitchd|Slipkey)" | grep -v grep
```
Expected: only the Slipkey process. **No imeswitchd child.**

- [ ] **Step 7: If anything fails**

Common issues and what to check:
- Hook log line missing → `EventHook.install()` returned an error; check the `log stream` for `hook install failed`. Most likely Accessibility wasn't actually granted to the new bundle (re-check identity matches).
- Switch logs but IME doesn't actually change → `IMEManager.select` fell through; check the source ID in `~/.config/imeswitch/config.toml` matches an enabled selectable input source (`Slipkey.app/Contents/MacOS/Slipkey ...` won't help here; use the standalone `cargo run -p imeswitchd -- list` from the repo to dump installed sources).
- Triggers do nothing, no log lines on keydown → CGEventTap created but isn't receiving events. Re-grant Accessibility and confirm the bundle codesign identity hasn't changed since the grant.

If verification passes, commit a verification note (no code change):

```bash
git commit --allow-empty -m "chore: end-to-end native hook verified"
```

---

## Task 12: Delete the dead `imeswitch-macos` Rust crate

**Files:**
- Delete: `crates/imeswitch-macos/`
- Modify: `Cargo.toml` (workspace) — remove `imeswitch-macos` member
- Modify: `bins/imeswitchd/Cargo.toml` — drop the macOS-conditional dependency on `imeswitch-macos`
- Modify: `bins/imeswitchd/src/main.rs` — remove the `#[cfg(target_os = "macos")]` impl entirely

This task removes ~1000 lines of now-dead Rust. The `imeswitchd` binary becomes Windows-only at the source level. We retain the binary for Windows users.

- [ ] **Step 1: Inspect the workspace Cargo.toml**

```bash
cat /Users/zlb/Desktop/imeswitch/Cargo.toml
```
Note which entries reference `crates/imeswitch-macos`.

- [ ] **Step 2: Remove `imeswitch-macos` from workspace members**

Edit `/Users/zlb/Desktop/imeswitch/Cargo.toml`. Remove the `"crates/imeswitch-macos"` line from the `members` array.

- [ ] **Step 3: Inspect imeswitchd Cargo.toml**

```bash
cat /Users/zlb/Desktop/imeswitch/bins/imeswitchd/Cargo.toml
```

Find any `[target.'cfg(target_os = "macos")'.dependencies]` block and delete the entire block.

- [ ] **Step 4: Trim main.rs**

Open `bins/imeswitchd/src/main.rs` and delete:
- The `#[cfg(target_os = "macos")] fn run()` block (about lines 1-61)
- The `#[cfg(target_os = "macos")] fn list_sources()` block
- The `#[cfg(target_os = "macos")] fn init_config()` block
- The `#[cfg(target_os = "macos")] fn wizard_config()` block

Update `print_usage()` to mention that macOS support moved to the Slipkey app:

```rust
fn print_usage() {
    eprintln!("usage: imeswitchd [SUBCOMMAND]");
    eprintln!();
    eprintln!("On macOS, IME switching now lives inside the Slipkey app — this");
    eprintln!("CLI is Windows-only.");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  (none)  run the daemon (default; Windows only)");
    eprintln!("  list    print all keyboard layouts known to the OS (Windows only)");
    eprintln!("  init    write a starter config file (Windows only)");
}
```

And in `main()`, drop the `wizard` arm (it's macOS-only):

```rust
        Some("wizard") => {
            eprintln!("wizard is no longer supported in this CLI; use the Slipkey app");
            std::process::exit(1);
        }
```

- [ ] **Step 5: Delete the macOS crate**

```bash
rm -rf crates/imeswitch-macos
```

- [ ] **Step 6: Verify the workspace still builds for both targets**

```bash
cargo check --workspace
cargo check -p imeswitchd --target x86_64-pc-windows-msvc
```
Expected: both succeed. macOS workspace check should not try to build any imeswitch-macos code.

- [ ] **Step 7: Run remaining Rust tests**

```bash
cargo test --workspace
```
Expected: imeswitch-core tests pass; imeswitchd has no macOS-specific tests anymore.

- [ ] **Step 8: Commit**

```bash
git add -A crates/imeswitch-macos Cargo.toml bins/imeswitchd
git commit -m "refactor: delete imeswitch-macos crate (logic moved to Slipkey)"
```

---

## Task 13: Update CLAUDE.md to reflect the new architecture

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Rewrite the relevant sections of CLAUDE.md**

Replace the "Crate layout" section to reflect the deletion of `imeswitch-macos`:

```markdown
### Crate layout

- `crates/imeswitch-core`: platform-agnostic state machine. Used by the Windows daemon. The macOS Slipkey app has its own (Swift) port of this.
- `crates/imeswitch-windows`: Windows daemon implementation. Depends on `imeswitch-core`.
- `bins/imeswitchd`: Windows-only daemon binary. macOS no longer uses this.
- `bins/slipkey-app`: macOS native app (Swift). Hosts the event hook, IME switcher, and settings UI in the main process — no subprocess.
```

Replace the "macOS specifics" section header and contents to point readers at `bins/slipkey-app/Sources/SlipkeyApp/Hook/` instead of the deleted Rust crate.

Update the "Known open issues" section to remove the daemon-context items that no longer apply.

Update the "Commands" section to note the macOS dev workflow is `swift test --package-path bins/slipkey-app` and `bash scripts/package-macos.sh`.

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for native Slipkey hook architecture"
```

---

## Self-review checklist (run before handoff)

- [x] Every task lists exact file paths
- [x] Every TDD task has: write test → run/fail → implement → run/pass → commit
- [x] No "TBD"/"implement later" placeholders
- [x] Type names are consistent across tasks (`HookKey`, `StateMachine`, `StateMachineResponse`, `IMEManager`, `Composition`, `EventHook`, `HookService`)
- [x] All callers of removed types (`DaemonService`, `imeswitch-macos` crate) are updated in the same task that removes them, so the build never breaks mid-task
- [x] Spec coverage:
  - Single-binary architecture: Tasks 6-10
  - Mos UX parity: Task 8 (existing StatusItemManager / WindowManager unchanged)
  - State-machine parity with Rust: Task 3 (12 ported tests)
  - IME switching parity: Task 4 + Task 7
  - Composition heuristic parity: Task 5 (AX path; candidate-window deferred — documented)
  - End-to-end verification: Task 11
  - Cleanup: Tasks 12-13
