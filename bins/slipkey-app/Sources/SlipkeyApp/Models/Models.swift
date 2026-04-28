import Foundation

struct InputSource: Identifiable, Hashable {
    var id: String { sourceID }
    let language: String
    let sourceID: String
    let name: String
    let isSelectable: Bool
}

struct MappingEntry: Identifiable, Hashable {
    var id: String { language }
    var language: String
    var prefix: String
    var source: String
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

    func mergingDetectedSources(_ sources: [InputSource]) -> SlipkeyConfig {
        let languages = Array(Set(sources.filter(\.isSelectable).map(\.language))).sorted()
        guard !languages.isEmpty else { return self }
        let rows = languages.map { language -> MappingEntry in
            let candidates = sources.filter { $0.language == language && $0.isSelectable }
            let existing = mappings.first { item in
                item.language == language && candidates.contains { $0.sourceID == item.source }
            } ?? mappings.first { $0.language == language }
            return MappingEntry(
                language: language,
                prefix: existing?.prefix ?? language.lowercased(),
                source: candidates.contains { $0.sourceID == existing?.source } ? (existing?.source ?? "") : (candidates.first?.sourceID ?? existing?.source ?? "")
            )
        }
        return SlipkeyConfig(leader: leader, mappings: rows)
    }
}
