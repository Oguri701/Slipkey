use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language(String);

impl Language {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into().to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Language {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Language {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Logical key recognized by the trigger state machine.
///
/// Maps one-to-one onto physical keys on a US-QWERTY layout.
/// Any key we don't care about is `Other`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// `;` — the leader symbol.
    Leader,
    AlphaNum(char),
    Other,
}

impl Key {
    pub fn alpha_num(value: char) -> Self {
        if value.is_ascii_alphanumeric() {
            Key::AlphaNum(value.to_ascii_lowercase())
        } else {
            Key::Other
        }
    }
}
