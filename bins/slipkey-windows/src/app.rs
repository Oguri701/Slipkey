use std::sync::{Arc, Mutex};

use imeswitch_windows::config::{load_or_default, Config, LoadOutcome, MappingConfig};
use imeswitch_windows::ime::{detect_default_sources, SourceInfo};

pub struct AppState {
    pub config: Config,
    pub detected_sources: Vec<SourceInfo>,
    pub status_message: String,
    pub hook_active: bool,
    pub launch_at_login: bool,
    pub ui_language: String,
}

impl AppState {
    pub fn load() -> Self {
        let (_mapping, outcome) = load_or_default();
        let detected_sources = detect_default_sources();
        let base_config = match outcome {
            LoadOutcome::Loaded { config, .. } | LoadOutcome::Migrated { config, .. } => config,
            LoadOutcome::Missing { .. } | LoadOutcome::ParseError { .. } => empty_detected_config(),
        };
        let ui_language = base_config.normalized_ui_language();
        let config = merge_detected_sources(base_config, &detected_sources);
        AppState {
            config,
            detected_sources,
            status_message: String::new(),
            hook_active: false,
            launch_at_login: crate::startup::is_enabled(),
            ui_language,
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn empty_detected_config() -> Config {
    Config {
        leader: Some(";".to_string()),
        ui_language: Some("en".to_string()),
        mappings: Some(Vec::new()),
        en: None,
        ja: None,
        zh: None,
    }
}

/// Rebuild mapping rows from detected keyboard layouts.
///
/// Phase 2 keeps one row per language. If a language has multiple sources,
/// the row stores the selected source and the UI exposes the candidates in a
/// dropdown. Existing prefixes and selected sources are preserved across
/// repeated Detect operations.
pub fn merge_detected_sources(mut config: Config, sources: &[SourceInfo]) -> Config {
    let existing = config
        .mappings
        .take()
        .unwrap_or_else(|| Config::default().mappings.unwrap_or_default());
    let defaults = Config::default().mappings.unwrap_or_default();

    let mut languages: Vec<String> = existing
        .iter()
        .map(|entry| entry.language.clone())
        .chain(
            sources
                .iter()
                .filter(|source| source.is_selectable)
                .map(|source| source.language.clone()),
        )
        .collect();
    languages.sort();
    languages.dedup();

    languages.sort_by_key(|language| language_sort_key(language));

    let mut rows = Vec::new();
    for language in languages {
        let candidates: Vec<&SourceInfo> = sources
            .iter()
            .filter(|source| source.language == language && source.is_selectable)
            .collect();
        let existing_entry = existing.iter().find(|entry| entry.language == language);
        let default_entry = defaults.iter().find(|entry| entry.language == language);
        let selected_source = preferred_source(&language, &candidates, existing_entry);

        let source = if language == "en" {
            None
        } else {
            selected_source
                .map(|source| source.id.clone())
                .or_else(|| existing_entry.and_then(|entry| entry.source.clone()))
                .or_else(|| default_entry.and_then(|entry| entry.source.clone()))
        };
        let name = selected_source
            .map(|source| source.name.clone())
            .or_else(|| existing_entry.map(|entry| entry.name.clone()))
            .unwrap_or_default();

        rows.push(MappingConfig {
            language: language.clone(),
            prefix: existing_entry
                .or(default_entry)
                .map(|entry| entry.prefix.clone())
                .unwrap_or_else(|| language.to_ascii_lowercase()),
            source,
            name,
            enabled: true,
        });
    }

    config.mappings = Some(rows);
    config
}

fn language_sort_key(language: &str) -> (usize, String) {
    let rank = match language {
        "en" => 0,
        "ja" => 1,
        "zh" => 2,
        "ko" => 3,
        _ => 10,
    };
    (rank, language.to_string())
}

fn preferred_source<'a>(
    language: &str,
    candidates: &'a [&'a SourceInfo],
    existing: Option<&MappingConfig>,
) -> Option<&'a SourceInfo> {
    if let Some(existing) = existing {
        if let Some(source) = existing.source.as_deref() {
            if let Some(candidate) = candidates.iter().copied().find(|item| item.id == source) {
                return Some(candidate);
            }
        }
    }

    let preferred_names = preferred_source_names(language);
    candidates
        .iter()
        .copied()
        .find(|source| {
            let name = source.name.to_ascii_lowercase();
            preferred_names
                .iter()
                .any(|preferred| name.contains(&preferred.to_ascii_lowercase()))
        })
        .or_else(|| candidates.first().copied())
}

fn preferred_source_names(language: &str) -> &'static [&'static str] {
    match language {
        "en" => &["US", "English"],
        "ja" => &["Japanese - Romaji", "Microsoft Japanese IME", "Japanese"],
        "zh" => &["Microsoft Pinyin", "Pinyin", "Shuangpin", "Chinese"],
        "ko" => &["Korean"],
        "fr" => &["French"],
        "de" => &["German"],
        "es" => &["Spanish"],
        "it" => &["Italian"],
        "ru" => &["Russian"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, language: &str) -> SourceInfo {
        SourceInfo {
            platform: "windows".to_string(),
            id: id.to_string(),
            name: language.to_string(),
            raw_language: language.to_string(),
            language: language.to_string(),
            is_selectable: true,
        }
    }

    #[test]
    fn merge_preserves_custom_non_detected_mappings() {
        let config = Config {
            leader: Some(";".to_string()),
            ui_language: None,
            mappings: Some(vec![
                MappingConfig {
                    language: "en".to_string(),
                    prefix: "e".to_string(),
                    source: None,
                    name: String::new(),
                    enabled: true,
                },
                MappingConfig {
                    language: "fr".to_string(),
                    prefix: "fr".to_string(),
                    source: Some("0000040C".to_string()),
                    name: "French".to_string(),
                    enabled: true,
                },
            ]),
            en: None,
            ja: None,
            zh: None,
        };

        let merged = merge_detected_sources(config, &[source("00000411", "ja")]);
        let mappings = merged.mappings.unwrap();

        assert!(mappings.iter().any(|mapping| mapping.language == "ja"));
        assert!(mappings.iter().any(|mapping| {
            mapping.language == "fr" && mapping.source.as_deref() == Some("0000040C")
        }));
    }

    #[test]
    fn merge_updates_detected_language_without_duplicating_existing_row() {
        let config = Config {
            leader: Some(";".to_string()),
            ui_language: None,
            mappings: Some(vec![MappingConfig {
                language: "ja".to_string(),
                prefix: "jp".to_string(),
                source: Some("old".to_string()),
                name: String::new(),
                enabled: true,
            }]),
            en: None,
            ja: None,
            zh: None,
        };

        let merged = merge_detected_sources(config, &[source("00000411", "ja")]);
        let mappings = merged.mappings.unwrap();
        let ja_rows = mappings
            .iter()
            .filter(|mapping| mapping.language == "ja")
            .collect::<Vec<_>>();

        assert_eq!(ja_rows.len(), 1);
        assert_eq!(ja_rows[0].prefix, "jp");
        assert_eq!(ja_rows[0].source.as_deref(), Some("00000411"));
    }

    #[test]
    fn merge_groups_multiple_sources_into_one_language_row() {
        let merged = merge_detected_sources(
            Config::default(),
            &[
                SourceInfo {
                    name: "Microsoft Pinyin".to_string(),
                    ..source("08040804", "zh")
                },
                SourceInfo {
                    name: "Shuangpin".to_string(),
                    ..source("E0200804", "zh")
                },
            ],
        );
        let mappings = merged.mappings.unwrap();
        let zh_rows = mappings
            .iter()
            .filter(|mapping| mapping.language == "zh")
            .collect::<Vec<_>>();
        assert_eq!(zh_rows.len(), 1);
        assert_eq!(zh_rows[0].source.as_deref(), Some("08040804"));
    }

    #[test]
    fn merge_adds_detected_non_cjk_language() {
        let merged = merge_detected_sources(empty_detected_config(), &[source("0000040C", "fr")]);
        let fr = merged
            .mappings
            .unwrap()
            .into_iter()
            .find(|mapping| mapping.language == "fr")
            .unwrap();
        assert_eq!(fr.prefix, "fr");
        assert_eq!(fr.source.as_deref(), Some("0000040C"));
        assert!(fr.enabled);
    }

    #[test]
    fn fresh_config_uses_detected_languages_only() {
        let merged = merge_detected_sources(empty_detected_config(), &[source("0000040C", "fr")]);
        let languages = merged
            .mappings
            .unwrap()
            .into_iter()
            .map(|mapping| mapping.language)
            .collect::<Vec<_>>();
        assert_eq!(languages, vec!["fr"]);
    }
}
