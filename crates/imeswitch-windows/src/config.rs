//! User-editable config at `%APPDATA%\imeswitch\config.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ime::Mapping;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub en: Option<String>,
    pub ja: Option<String>,
    pub zh: Option<String>,
}

impl Config {
    pub fn into_mapping(self) -> Mapping {
        let mut mapping = Mapping::default();
        if let Some(value) = self.en {
            mapping.en = value;
        }
        if let Some(value) = self.ja {
            mapping.ja = value;
        }
        if let Some(value) = self.zh {
            mapping.zh = value;
        }
        mapping
    }

    pub fn template_toml() -> String {
        let d = Mapping::default();
        format!(
            "# imeswitch Windows config - edit to match your installed layouts.\n\
             # Use `imeswitchd list` to see currently loaded HKLs.\n\
             \n\
             en = \"{en}\"\n\
             ja = \"{ja}\"\n\
             zh = \"{zh}\"\n",
            en = d.en,
            ja = d.ja,
            zh = d.zh,
        )
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

pub fn load_or_default() -> (Mapping, LoadOutcome) {
    let path = default_path();
    let outcome = load_from(&path);
    let mapping = match &outcome {
        LoadOutcome::Loaded { config, .. } => config.clone().into_mapping(),
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
    fn partial_config_overrides_only_present_keys() {
        let mapping = toml::from_str::<Config>(r#"zh = "E0200804""#)
            .unwrap()
            .into_mapping();
        let default = Mapping::default();
        assert_eq!(mapping.en, default.en);
        assert_eq!(mapping.ja, default.ja);
        assert_eq!(mapping.zh, "E0200804");
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
