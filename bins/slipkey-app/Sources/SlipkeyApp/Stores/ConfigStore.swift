import Foundation

enum ConfigStore {
    static var path: URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return home.appendingPathComponent(".config/imeswitch/config.toml")
    }

    static func load() -> SlipkeyConfig {
        guard let text = try? String(contentsOf: path, encoding: .utf8) else {
            return .defaults()
        }
        var leader = ";"
        var rows: [MappingEntry] = []
        var current: [String: String] = [:]

        func flush() {
            guard let language = current["language"], let source = current["source"] else {
                current.removeAll()
                return
            }
            rows.append(MappingEntry(language: language, prefix: current["prefix"] ?? language, source: source))
            current.removeAll()
        }

        for rawLine in text.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            if line == "[[mappings]]" {
                flush()
                continue
            }
            guard let eq = line.firstIndex(of: "=") else { continue }
            let key = line[..<eq].trimmingCharacters(in: .whitespaces)
            let value = unquote(String(line[line.index(after: eq)...]).trimmingCharacters(in: .whitespaces))
            if key == "leader" {
                leader = value
            } else if ["language", "prefix", "source"].contains(String(key)) {
                current[String(key)] = value
            }
        }
        flush()
        return SlipkeyConfig(leader: leader.isEmpty ? ";" : leader, mappings: rows.isEmpty ? SlipkeyConfig.defaults().mappings : rows)
    }

    static func save(_ config: SlipkeyConfig) throws {
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        var lines: [String] = [
            "leader = \(quote(String(config.leader.prefix(1))))",
            ""
        ]
        for mapping in config.mappings {
            lines.append("[[mappings]]")
            lines.append("language = \(quote(mapping.language))")
            lines.append("prefix = \(quote(mapping.prefix))")
            lines.append("source = \(quote(mapping.source))")
            lines.append("")
        }
        try lines.joined(separator: "\n").write(to: path, atomically: true, encoding: .utf8)
    }

    private static func quote(_ value: String) -> String {
        "\"\(value.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\""))\""
    }

    private static func unquote(_ value: String) -> String {
        var result = value
        if result.hasPrefix("\"") { result.removeFirst() }
        if result.hasSuffix("\"") { result.removeLast() }
        return result.replacingOccurrences(of: "\\\"", with: "\"").replacingOccurrences(of: "\\\\", with: "\\")
    }
}
