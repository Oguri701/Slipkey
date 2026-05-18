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
            let dedupeKey = src.id
            guard seen.insert(dedupeKey).inserted else { continue }
            result.append(InputSource(
                platform: "macos",
                language: language,
                sourceID: src.id,
                name: src.name,
                rawLanguage: src.languages.first ?? "",
                isSelectable: src.isSelectable
            ))
        }
        return result
    }

    private func isRealTypingSource(_ type: String) -> Bool {
        type == "TISTypeKeyboardLayout" || type == "TISTypeKeyboardInputMode"
    }

    private func normalizedSupportedLanguage(_ rawLanguage: String) -> String? {
        Self.normalizedLanguage(rawLanguage)
    }

    static func normalizedLanguage(_ rawLanguage: String) -> String? {
        let language = rawLanguage
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "_", with: "-")
            .lowercased()
        guard !language.isEmpty else { return nil }
        let code = language.split(separator: "-", omittingEmptySubsequences: true).first.map(String.init) ?? language
        switch code {
        case "en", "ja", "zh", "ko", "fr", "de", "es":
            return code
        default:
            return nil
        }
    }
}
