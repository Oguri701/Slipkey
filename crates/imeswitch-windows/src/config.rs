//! User-editable config at `%APPDATA%\imeswitch\config.toml`.

use std::path::{Path, PathBuf};

use imeswitch_core::Language;
use serde::{Deserialize, Serialize};

use crate::ime::{WinEntry, WinImeMode, WinMapping, DEFAULT_LEADER};
use crate::keymap::leader_vk_for;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub leader: Option<String>,
    pub mappings: Option<Vec<MappingConfig>>,
    // Legacy v1 per-language overrides (migrated to mappings on first load)
    pub en: Option<String>,
    pub ja: Option<String>,
    pub zh: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MappingConfig {
    pub language: String,
    pub prefix: String,
    /// HKL identifier for CJK and other non-English languages.
    /// `None` for English, which uses alphanumeric mode without switching layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_mapping(&WinMapping::default())
    }
}

impl MappingConfig {
    fn from_entry(entry: &WinEntry) -> Self {
        Self {
            language: entry.language.to_string(),
            prefix: entry.prefix.clone(),
            source: entry.hkl_id.clone(),
        }
    }

    fn into_entry(self) -> Option<WinEntry> {
        if self.language.len() != 2 {
            return None;
        }
        if !self.prefix.is_empty() && !self.prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let mode = WinImeMode::for_language(&self.language);
        // Alphanumeric entries (English) never need an HKL — discard any legacy source.
        let hkl_id = match mode {
            WinImeMode::Alphanumeric => None,
            WinImeMode::Native | WinImeMode::LayoutOnly => {
                self.source.filter(|s| !s.is_empty())
            }
        };
        Some(WinEntry {
            language: Language::from(self.language),
            prefix: self.prefix.to_ascii_lowercase(),
            hkl_id,
            mode,
        })
    }
}

impl Config {
    pub fn leader_char(&self) -> char {
        self.leader
            .as_deref()
            .and_then(|s| s.chars().next())
            .filter(|c| leader_vk_for(*c).is_some())
            .unwrap_or(DEFAULT_LEADER)
    }

    pub fn into_mapping(self) -> WinMapping {
        let leader = self.leader_char();
        if let Some(mappings) = self.mappings {
            let entries = mappings
                .into_iter()
                .filter_map(MappingConfig::into_entry)
                .collect::<Vec<_>>();
            return WinMapping::with_leader(leader, entries);
        }

        // Legacy v1: per-language source overrides
        let mut entries = WinMapping::default().entries().to_vec();
        for entry in &mut entries {
            // Alphanumeric entries (English) never switch layout; skip legacy override.
            if entry.mode == WinImeMode::Alphanumeric {
                continue;
            }
            let override_source = match entry.language.as_str() {
                "ja" => self.ja.as_ref(),
                "zh" => self.zh.as_ref(),
                _ => None,
            };
            if let Some(source) = override_source {
                entry.hkl_id = Some(source.clone());
            }
        }
        WinMapping::with_leader(leader, entries)
    }

    pub fn is_legacy_v1(&self) -> bool {
        self.mappings.is_none() && (self.en.is_some() || self.ja.is_some() || self.zh.is_some())
    }

    pub fn template_toml() -> String {
        toml::to_string_pretty(&Config::default()).expect("default config serializes")
    }

    pub fn from_mapping(mapping: &WinMapping) -> Self {
        Self {
            leader: Some(mapping.leader().to_string()),
            mappings: Some(
                mapping
                    .entries()
                    .iter()
                    .map(MappingConfig::from_entry)
                    .collect(),
            ),
            en: None,
            ja: None,
            zh: None,
        }
    }

    fn template_toml_for(config: &Config) -> String {
        toml::to_string_pretty(config).expect("config serializes")
    }
}

#[derive(Debug)]
pub enum LoadOutcome {
    Loaded {
        path: PathBuf,
        config: Config,
    },
    Missing {
        path: PathBuf,
    },
    Migrated {
        path: PathBuf,
        backup_path: PathBuf,
        config: Config,
    },
    ParseError {
        path: PathBuf,
        error: toml::de::Error,
    },
}

pub fn default_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("imeswitch")
        .join("config.toml")
}

pub fn load_from(path: &Path) -> LoadOutcome {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LoadOutcome::Missing {
            path: path.to_path_buf(),
        },
        Err(e) => {
            log::warn!("could not read {}: {} - using defaults", path.display(), e);
            LoadOutcome::Missing {
                path: path.to_path_buf(),
            }
        }
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(config) if config.is_legacy_v1() => migrate_v1(path, text, config),
            Ok(config) => LoadOutcome::Loaded {
                path: path.to_path_buf(),
                config,
            },
            Err(error) => LoadOutcome::ParseError {
                path: path.to_path_buf(),
                error,
            },
        },
    }
}

fn migrate_v1(path: &Path, original_text: String, legacy: Config) -> LoadOutcome {
    let mapping = legacy.into_mapping();
    let config = Config::from_mapping(&mapping);
    let backup_path = path.with_file_name("config.toml.v1.bak");

    if let Err(error) = std::fs::write(&backup_path, original_text) {
        log::warn!("could not write {}: {}", backup_path.display(), error);
    }
    if let Err(error) = std::fs::write(path, Config::template_toml_for(&config)) {
        log::warn!("could not migrate {}: {}", path.display(), error);
    }

    LoadOutcome::Migrated {
        path: path.to_path_buf(),
        backup_path,
        config,
    }
}

pub fn load_or_default() -> (WinMapping, LoadOutcome) {
    let path = default_path();
    let outcome = load_from(&path);
    let mapping = match &outcome {
        LoadOutcome::Loaded { config, .. } | LoadOutcome::Migrated { config, .. } => {
            config.clone().into_mapping()
        }
        LoadOutcome::Missing { .. } => WinMapping::default(),
        LoadOutcome::ParseError { path, error } => {
            log::warn!(
                "{} is malformed: {} - ignoring, using defaults",
                path.display(),
                error
            );
            WinMapping::default()
        }
    };
    (mapping, outcome)
}

/// Saves `config` to the default path (`%APPDATA%\imeswitch\config.toml`).
pub fn save(config: &Config) -> anyhow::Result<()> {
    save_to(config, &default_path())
}

/// Saves `config` to an explicit path (used in tests and the settings window).
pub fn save_to(config: &Config, path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context as _;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create config dir")?;
    }
    let toml = toml::to_string_pretty(config).context("serialize config")?;
    std::fs::write(path, toml).context("write config")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let mapping = toml::from_str::<Config>("").unwrap().into_mapping();
        assert_eq!(mapping, WinMapping::default());
    }

    #[test]
    fn v2_config_supports_arbitrary_language() {
        let mapping = toml::from_str::<Config>(
            r#"
leader = ";"

[[mappings]]
language = "fr"
prefix = "fr"
source = "0000040C"
"#,
        )
        .unwrap()
        .into_mapping();
        let fr = mapping.entry_for(&Language::from("fr")).unwrap();
        assert_eq!(fr.hkl_id.as_deref(), Some("0000040C"));
        assert_eq!(fr.mode, WinImeMode::LayoutOnly);
    }

    #[test]
    fn english_entry_has_no_hkl_even_if_source_in_config() {
        // Legacy or manually-added source for English is ignored in the new model.
        let mapping = toml::from_str::<Config>(
            r#"
leader = ";"

[[mappings]]
language = "en"
prefix = "en"
source = "00000409"
"#,
        )
        .unwrap()
        .into_mapping();
        let en = mapping.entry_for(&Language::from("en")).unwrap();
        assert_eq!(en.hkl_id, None);
        assert_eq!(en.mode, WinImeMode::Alphanumeric);
    }

    #[test]
    fn legacy_config_overrides_only_present_cjk_keys() {
        let mapping = toml::from_str::<Config>(r#"zh = "E0200804""#)
            .unwrap()
            .into_mapping();
        let default = WinMapping::default();
        assert_eq!(
            mapping.entry_for(&Language::from("en")),
            default.entry_for(&Language::from("en"))
        );
        assert_eq!(
            mapping.entry_for(&Language::from("zh")).unwrap().hkl_id.as_deref(),
            Some("E0200804")
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>(r#"fr = "0000040C""#).is_err());
    }

    #[test]
    fn template_round_trips() {
        let parsed = toml::from_str::<Config>(&Config::template_toml()).unwrap();
        assert_eq!(parsed.into_mapping(), WinMapping::default());
    }

    #[test]
    fn save_to_and_reload_round_trips() {
        let tmp = std::env::temp_dir().join("imeswitch-test-save-round-trip.toml");
        let mapping = WinMapping::default();
        let config = Config::from_mapping(&mapping);
        save_to(&config, &tmp).expect("save_to failed");
        match load_from(&tmp) {
            LoadOutcome::Loaded { config: loaded, .. } => {
                assert_eq!(loaded.into_mapping(), mapping);
            }
            other => panic!("expected Loaded, got {:?}", other),
        }
        std::fs::remove_file(&tmp).ok();
    }
}
