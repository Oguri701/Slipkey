use std::sync::{Arc, Mutex};

use imeswitch_windows::config::{load_or_default, Config, MappingConfig};
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
        let (mapping, outcome) = load_or_default();
        let detected_sources = detect_default_sources();
        let base_config = match outcome {
            imeswitch_windows::config::LoadOutcome::Loaded { config, .. }
            | imeswitch_windows::config::LoadOutcome::Migrated { config, .. } => config,
            _ => Config::from_mapping(&mapping),
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

/// Rebuild the config's mapping rows from detected keyboard layouts.
///
/// English is always included as a mode-only entry (no HKL) regardless of
/// what layouts are installed. Detected rows are refreshed from `sources`;
/// user-defined rows for other languages are preserved.
pub fn merge_detected_sources(mut config: Config, sources: &[SourceInfo]) -> Config {
    let existing = config
        .mappings
        .take()
        .unwrap_or_else(|| Config::default().mappings.unwrap_or_default());
    let defaults = Config::default().mappings.unwrap_or_default();

    let mut detected_languages: Vec<String> = sources
        .iter()
        .filter(|source| source.language != "en")
        .map(|source| source.language.clone())
        .collect();
    detected_languages.sort();
    detected_languages.dedup();

    let mut rows = Vec::new();

    for language in ["en", "ja", "zh"] {
        let candidates: Vec<&SourceInfo> = sources
            .iter()
            .filter(|source| source.language == language)
            .collect();
        let selected = existing
            .iter()
            .find(|entry| {
                entry.language == language
                    && entry
                        .source
                        .as_deref()
                        .map_or(false, |id| candidates.iter().any(|source| source.id == id))
            })
            .or_else(|| existing.iter().find(|entry| entry.language == language))
            .or_else(|| defaults.iter().find(|entry| entry.language == language));

        let source = selected
            .filter(|entry| {
                entry.source.as_deref().map_or(false, |id| {
                    candidates.iter().any(|candidate| candidate.id == id)
                })
            })
            .and_then(|entry| entry.source.clone())
            .or_else(|| candidates.first().map(|source| source.id.clone()))
            .or_else(|| selected.and_then(|entry| entry.source.clone()));

        rows.push(MappingConfig {
            language: language.to_string(),
            prefix: selected
                .map(|entry| entry.prefix.clone())
                .unwrap_or_else(|| language.to_ascii_lowercase()),
            source: if language == "en" { None } else { source },
        });
    }

    for language in detected_languages
        .iter()
        .filter(|language| !matches!(language.as_str(), "ja" | "zh"))
    {
        let candidates: Vec<&SourceInfo> = sources
            .iter()
            .filter(|source| source.language == *language)
            .collect();
        let selected = existing.iter().find(|entry| entry.language == *language);
        rows.push(MappingConfig {
            language: language.clone(),
            prefix: selected
                .map(|entry| entry.prefix.clone())
                .unwrap_or_else(|| language.to_ascii_lowercase()),
            source: selected
                .and_then(|entry| entry.source.clone())
                .or_else(|| candidates.first().map(|source| source.id.clone())),
        });
    }

    for entry in existing.iter().filter(|entry| {
        !matches!(entry.language.as_str(), "en" | "ja" | "zh")
            && !detected_languages.contains(&entry.language)
    }) {
        rows.push(entry.clone());
    }

    config.mappings = Some(rows);
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, language: &str) -> SourceInfo {
        SourceInfo {
            id: id.to_string(),
            name: language.to_string(),
            language: language.to_string(),
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
                },
                MappingConfig {
                    language: "fr".to_string(),
                    prefix: "fr".to_string(),
                    source: Some("0000040C".to_string()),
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
}
