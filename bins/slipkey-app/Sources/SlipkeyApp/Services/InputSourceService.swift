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
