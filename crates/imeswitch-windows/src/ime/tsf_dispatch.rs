//! Host-side TSF dispatch: inject helper DLL, signal it, wait for completion.

use crate::ime::WinImeMode;

/// TSF conversion mode bits, mirror values from `<msctf.h>`.
pub const TF_CONVERSIONMODE_ALPHANUMERIC: u32 = 0x0000;
pub const TF_CONVERSIONMODE_NATIVE: u32 = 0x0001;
pub const TF_CONVERSIONMODE_FULLSHAPE: u32 = 0x0008;
pub const TF_CONVERSIONMODE_ROMAN: u32 = 0x0010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsfTarget {
    pub conversion_mode: u32,
    pub open_status: bool,
}

impl TsfTarget {
    /// Translate a (mode, language) pair into the TSF Compartment values.
    pub fn for_mode(mode: WinImeMode, language: &str) -> Option<Self> {
        match mode {
            WinImeMode::Alphanumeric => Some(Self {
                conversion_mode: TF_CONVERSIONMODE_ALPHANUMERIC,
                // Keep the IME active; only switch its internal mode.
                // This is the decision D1 from the design doc.
                open_status: true,
            }),
            WinImeMode::Native => Some(Self {
                conversion_mode: match language {
                    // Japanese needs full-shape + Roman input style for "ja kana via romaji".
                    "ja" => {
                        TF_CONVERSIONMODE_NATIVE
                            | TF_CONVERSIONMODE_FULLSHAPE
                            | TF_CONVERSIONMODE_ROMAN
                    }
                    // Chinese/Korean: just native. No full-shape forcing.
                    _ => TF_CONVERSIONMODE_NATIVE,
                },
                open_status: true,
            }),
            // LayoutOnly bypasses TSF entirely (e.g. French AZERTY).
            WinImeMode::LayoutOnly => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphanumeric_keeps_ime_open_and_uses_zero_mode() {
        let t = TsfTarget::for_mode(WinImeMode::Alphanumeric, "ja").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_ALPHANUMERIC);
        assert!(t.open_status, "must keep IME open (design D1)");
    }

    #[test]
    fn native_japanese_uses_native_fullshape_roman() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ja").unwrap();
        assert_eq!(
            t.conversion_mode,
            TF_CONVERSIONMODE_NATIVE | TF_CONVERSIONMODE_FULLSHAPE | TF_CONVERSIONMODE_ROMAN
        );
        assert!(t.open_status);
    }

    #[test]
    fn native_chinese_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "zh").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
        assert!(t.open_status);
    }

    #[test]
    fn native_korean_uses_native_only() {
        let t = TsfTarget::for_mode(WinImeMode::Native, "ko").unwrap();
        assert_eq!(t.conversion_mode, TF_CONVERSIONMODE_NATIVE);
    }

    #[test]
    fn layout_only_returns_none() {
        assert!(TsfTarget::for_mode(WinImeMode::LayoutOnly, "fr").is_none());
    }
}
