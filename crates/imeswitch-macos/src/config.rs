//! User-editable config at `$HOME/.config/imeswitch/config.toml`.
//!
//! Every field is optional — missing keys fall back to `Mapping::default()`,
//! which targets the three Apple-bundled sources (ABC / Kotoeri Romaji
//! Japanese / SCIM Shuangpin). Users on other IMEs (全拼, Rime, 搜狗, ATOK)
//! replace the relevant string; `imeswitchd list` shows the legal IDs.

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
    /// Merge this config onto the built-in defaults to produce a full Mapping.
    pub fn into_mapping(self) -> Mapping {
        let mut m = Mapping::default();
        // Apply overrides; leak short-lived Strings to &'static because
        // Mapping's fields are &'static str. This is fine because Config is
        // loaded once at process start and lives for the entire run.
        if let Some(s) = self.en {
            m.en = Box::leak(s.into_boxed_str());
        }
        if let Some(s) = self.ja {
            m.ja = Box::leak(s.into_boxed_str());
        }
        if let Some(s) = self.zh {
            m.zh = Box::leak(s.into_boxed_str());
        }
        m
    }

    /// A TOML-serialized template with sensible defaults filled in. Used by
    /// `imeswitchd init` so users start from a working config rather than an
    /// empty file.
    pub fn template_toml() -> String {
        let d = Mapping::default();
        format!(
            "# imeswitch config — edit to match your installed IMEs.\n\
             # Use `imeswitchd list` to see every source's real ID.\n\
             # Remove a line to fall back to the built-in default shown here.\n\
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
    Loaded { path: PathBuf, config: Config },
    Missing { path: PathBuf },
    ParseError { path: PathBuf, error: toml::de::Error },
}

/// Path we look for the config at. Respects `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("imeswitch").join("config.toml")
}

pub fn load_from(path: &Path) -> LoadOutcome {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            LoadOutcome::Missing { path: path.to_path_buf() }
        }
        Err(e) => {
            // Treat unreadable (permission etc.) like missing so we never
            // refuse to start; log the underlying error.
            log::warn!("could not read {}: {} — using defaults", path.display(), e);
            LoadOutcome::Missing { path: path.to_path_buf() }
        }
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(c) => LoadOutcome::Loaded { path: path.to_path_buf(), config: c },
            Err(e) => LoadOutcome::ParseError { path: path.to_path_buf(), error: e },
        },
    }
}

/// Convenience: load from the default path, return the effective Mapping.
/// Errors are surfaced as log warnings; we always return a usable Mapping.
pub fn load_or_default() -> (Mapping, LoadOutcome) {
    let path = default_path();
    let outcome = load_from(&path);
    let mapping = match &outcome {
        LoadOutcome::Loaded { config, .. } => config.clone().into_mapping(),
        LoadOutcome::Missing { .. } => Mapping::default(),
        LoadOutcome::ParseError { path, error } => {
            log::warn!(
                "{} is malformed: {} — ignoring, using defaults",
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
    fn empty_config_is_default_mapping() {
        let c: Config = toml::from_str("").unwrap();
        let m = c.into_mapping();
        let d = Mapping::default();
        assert_eq!(m.en, d.en);
        assert_eq!(m.ja, d.ja);
        assert_eq!(m.zh, d.zh);
    }

    #[test]
    fn partial_config_only_overrides_given_keys() {
        let c: Config = toml::from_str(r#"zh = "com.example.fake""#).unwrap();
        let m = c.into_mapping();
        let d = Mapping::default();
        assert_eq!(m.en, d.en);
        assert_eq!(m.ja, d.ja);
        assert_eq!(m.zh, "com.example.fake");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let r: Result<Config, _> = toml::from_str(r#"pl = "polish""#);
        assert!(r.is_err(), "unknown keys should fail deserialization");
    }

    #[test]
    fn template_round_trips() {
        let tpl = Config::template_toml();
        let c: Config = toml::from_str(&tpl).unwrap();
        let d = Mapping::default();
        assert_eq!(c.en.as_deref(), Some(d.en));
        assert_eq!(c.ja.as_deref(), Some(d.ja));
        assert_eq!(c.zh.as_deref(), Some(d.zh));
    }
}
