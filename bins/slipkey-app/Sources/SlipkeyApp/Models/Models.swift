import Foundation

struct InputSource: Identifiable, Hashable {
    var id: String { sourceID }
    let platform: String
    let language: String
    let sourceID: String
    let name: String
    let rawLanguage: String
    let isSelectable: Bool

    init(
        platform: String = "macos",
        language: String,
        sourceID: String,
        name: String,
        rawLanguage: String = "",
        isSelectable: Bool
    ) {
        self.platform = platform
        self.language = language
        self.sourceID = sourceID
        self.name = name
        self.rawLanguage = rawLanguage
        self.isSelectable = isSelectable
    }
}

struct MappingEntry: Identifiable, Hashable {
    var id: String { language }
    var language: String
    var prefix: String
    var source: String
    var name: String
    var enabled: Bool

    init(language: String, prefix: String, source: String, name: String = "", enabled: Bool = true) {
        self.language = language
        self.prefix = prefix
        self.source = source
        self.name = name
        self.enabled = enabled
    }
}

struct SlipkeyConfig: Hashable {
    var leader: String
    var mappings: [MappingEntry]

    static func defaults() -> SlipkeyConfig {
        SlipkeyConfig(
            leader: ";",
            mappings: [
                MappingEntry(language: "en", prefix: "en", source: "com.apple.keylayout.ABC"),
                MappingEntry(language: "ja", prefix: "ja", source: "com.apple.inputmethod.Kotoeri.RomajiTyping.Japanese"),
                MappingEntry(language: "zh", prefix: "zh", source: "com.apple.inputmethod.SCIM.Shuangpin")
            ]
        )
    }

    func validationErrors() -> [String] {
        var errors: [String] = []
        let rows = mappings
            .filter(\.enabled)
            .map { ($0.language, $0.prefix.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()) }
            .filter { !$0.1.isEmpty }

        for (_, prefix) in rows {
            if prefix.contains(where: { character in
                if case .alphaNum = HookKey.from(character: character) {
                    return false
                }
                return true
            }) {
                errors.append("Prefixes can only contain letters and numbers.")
            }
        }

        var seen: [String: String] = [:]
        for (language, prefix) in rows {
            if seen[prefix] != nil {
                errors.append("Prefixes must be unique.")
            } else {
                seen[prefix] = language
            }
        }

        for (index, row) in rows.enumerated() {
            for other in rows.dropFirst(index + 1) {
                if row.1 != other.1 && (row.1.hasPrefix(other.1) || other.1.hasPrefix(row.1)) {
                    errors.append("Prefixes cannot start with another configured prefix.")
                    return errors
                }
            }
        }

        return errors
    }

    func mergingDetectedSources(_ sources: [InputSource]) -> SlipkeyConfig {
        let selectableSources = sources.filter(\.isSelectable)
        guard !selectableSources.isEmpty else { return self }
        let existingLanguages = Set(mappings.map(\.language))
        let detectedLanguages = Set(selectableSources.map(\.language))
        let languages = Array(existingLanguages.union(detectedLanguages)).sorted()

        let rows = languages.map { language -> MappingEntry in
            let candidates = selectableSources.filter { $0.language == language }
            let existing = mappings.first { $0.language == language }
            let selected = preferredSource(for: language, candidates: candidates, existing: existing)
            return MappingEntry(
                language: language,
                prefix: existing?.prefix ?? language.lowercased(),
                source: selected?.sourceID ?? existing?.source ?? "",
                name: selected?.name ?? existing?.name ?? "",
                enabled: existing?.enabled ?? true
            )
        }
        return SlipkeyConfig(leader: leader, mappings: rows)
    }

    private func preferredSource(for language: String, candidates: [InputSource], existing: MappingEntry?) -> InputSource? {
        if let existing, let match = candidates.first(where: { $0.sourceID == existing.source }) {
            return match
        }
        let preferredNames = Self.preferredSourceNames[language] ?? [language]
        if let match = candidates.first(where: { source in
            let sourceName = source.name.lowercased()
            return preferredNames.contains { sourceName.contains($0.lowercased()) }
        }) {
            return match
        }
        return candidates.first
    }

    private static let preferredSourceNames: [String: [String]] = [
        "en": ["ABC", "US", "English"],
        "ja": ["Japanese - Romaji", "Microsoft Japanese IME", "Japanese"],
        "zh": ["Microsoft Pinyin", "Pinyin", "Shuangpin"],
        "ko": ["Korean"],
        "fr": ["French"],
        "de": ["German"],
        "es": ["Spanish"]
    ]
}
