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
