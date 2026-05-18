import Foundation

struct InputSourceService {
    func discover() -> [InputSource] {
        var result: [InputSource] = []
        for src in IMEManager.listAll() {
            guard isRealTypingSource(src.type),
                  src.category == "TISCategoryKeyboardInputSource",
                  src.isEnabled,
                  src.isSelectable
            else { continue }
            guard let language = src.languages.compactMap(normalizedSupportedLanguage).first
            else { continue }
            let candidate = InputSource(
                platform: "macos",
                language: language,
                sourceID: src.id,
                name: src.name,
                rawLanguage: src.languages.first ?? "",
                isSelectable: src.isSelectable
            )
            insertDeduped(candidate, into: &result)
        }
        return result
    }

    private func isRealTypingSource(_ type: String) -> Bool {
        type == "TISTypeKeyboardLayout" || type == "TISTypeKeyboardInputMode"
    }

    private func normalizedSupportedLanguage(_ rawLanguage: String) -> String? {
        Self.normalizedLanguage(rawLanguage)
    }

    private func insertDeduped(_ candidate: InputSource, into result: inout [InputSource]) {
        let duplicateIndex = result.firstIndex { existing in
            existing.language == candidate.language &&
            existing.name.caseInsensitiveCompare(candidate.name) == .orderedSame
        }
        guard let duplicateIndex else {
            result.append(candidate)
            return
        }
        if Self.prefers(candidate, over: result[duplicateIndex]) {
            result[duplicateIndex] = candidate
        }
    }

    static func dedupedForDisplay(_ sources: [InputSource]) -> [InputSource] {
        var result: [InputSource] = []
        let service = InputSourceService()
        for source in sources {
            service.insertDeduped(source, into: &result)
        }
        return result
    }

    private static func prefers(_ candidate: InputSource, over existing: InputSource) -> Bool {
        sourcePriority(candidate) < sourcePriority(existing)
    }

    private static func sourcePriority(_ source: InputSource) -> Int {
        let id = source.sourceID.lowercased()
        if id.contains("romajityping") { return 0 }
        if id.contains("kanatyping") { return 10 }
        return 5
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
