//! User-editable config at `%APPDATA%\imeswitch\config.toml`.

use std::path::{Path, PathBuf};

use imeswitch_core::Language;
use serde::{Deserialize, Serialize};

use crate::ime::{Mapping, MappingEntry, DEFAULT_LEADER};
use crate::keymap::leader_vk_for;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub leader: Option<String>,
    pub mappings: Option<Vec<MappingConfig>>,
    pub en: Option<String>,
    pub ja: Option<String>,
    pub zh: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MappingConfig {
    pub language: String,
    pub prefix: String,
    pub source: String,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_mapping(&Mapping::default())
    }
}

impl MappingConfig {
    fn from_entry(entry: &MappingEntry) -> Self {
        Self {
            language: entry.language.to_string(),
            prefix: entry.prefix.clone(),
            source: entry.source.clone(),
        }
    }

    fn into_entry(self) -> Option<MappingEntry> {
        if self.language.len() != 2 {
            return None;
        }
        if !self.prefix.is_empty() && !self.prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(MappingEntry {
            language: Language::from(self.language),
            prefix: self.prefix.to_ascii_lowercase(),
            source: self.source,
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

    pub fn into_mapping(self) -> Mapping {
        let leader = self.leader_char();
        if let Some(mappings) = self.mappings {
            let entries = mappings
                .into_iter()
                .filter_map(MappingConfig::into_entry)
                .collect::<Vec<_>>();
            return Mapping::with_leader(leader, entries);
        }

        let mut entries = Mapping::default().entries().to_vec();
        for entry in &mut entries {
            let override_source = match entry.language.as_str() {
                "en" => self.en.as_ref(),
                "ja" => self.ja.as_ref(),
                "zh" => self.zh.as_ref(),
                _ => None,
            };
            if let Some(source) = override_source {
                entry.source = source.clone();
            }
        }
        Mapping::with_leader(leader, entries)
    }

    pub fn is_legacy_v1(&self) -> bool {
        self.mappings.is_none() && (self.en.is_some() || self.ja.is_some() || self.zh.is_some())
    }

    pub fn template_toml() -> String {
        toml::to_string_pretty(&Config::default()).expect("default config serializes")
    }

    pub fn from_mapping(mapping: &Mapping) -> Self {
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

pub fn load_or_default() -> (Mapping, LoadOutcome) {
    let path = default_path();
    let outcome = load_from(&path);
    let mapping = match &outcome {
        LoadOutcome::Loaded { config, .. } | LoadOutcome::Migrated { config, .. } => {
            config.clone().into_mapping()
        }
        LoadOutcome::Missing { .. } => Mapping::default(),
        LoadOutcome::ParseError { path, error } => {
            log::warn!(
                "{} is malformed: {} - ignoring, using defaults",
                path.display(),
                error
            );
            Mapping::default()
        }
    };
    (mapping, outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_uses_defaults() {
        let mapping = toml::from_str::<Config>("").unwrap().into_mapping();
        assert_eq!(mapping, Mapping::default());
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
        assert_eq!(mapping.source_for(&Language::from("fr")), Some("0000040C"));
    }

    #[test]
    fn legacy_config_overrides_only_present_keys() {
        let mapping = toml::from_str::<Config>(r#"zh = "E0200804""#)
            .unwrap()
            .into_mapping();
        let default = Mapping::default();
        assert_eq!(
            mapping.source_for(&Language::from("en")),
            default.source_for(&Language::from("en"))
        );
        assert_eq!(mapping.source_for(&Language::from("zh")), Some("E0200804"));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<Config>(r#"fr = "0000040C""#).is_err());
    }

    #[test]
    fn template_round_trips() {
        let parsed = toml::from_str::<Config>(&Config::template_toml()).unwrap();
        assert_eq!(parsed.into_mapping(), Mapping::default());
    }
}
