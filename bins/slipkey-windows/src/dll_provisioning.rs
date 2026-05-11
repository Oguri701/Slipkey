//! Provision the bundled helper DLL into a stable location at runtime so
//! tray-app users do not need to keep an external slipkey_tsf.dll next to
//! Slipkey.exe.
//!
//! Layout:
//!   %LOCALAPPDATA%\Slipkey\slipkey_tsf.dll -> target
//!   <embedded bytes from bins/slipkey-windows/embed/slipkey_tsf.dll>
//!
//! On every launch we hash the on-disk DLL with SHA256 and compare with the
//! hash of EMBEDDED_DLL. Mismatch (first run, upgrade, tampering) triggers a
//! rewrite. Equal: leave the file as-is, return path.

use std::path::{Path, PathBuf};

/// The helper DLL bytes baked into the EXE at compile time.
pub const EMBEDDED_DLL: &[u8] = include_bytes!("../embed/slipkey_tsf.dll");

/// File name written under %LOCALAPPDATA%\Slipkey\.
const DLL_FILE_NAME: &str = "slipkey_tsf.dll";

#[derive(Debug)]
pub enum ProvisionError {
    NoLocalAppData,
    Io(std::io::Error),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLocalAppData => write!(f, "LOCALAPPDATA environment variable is unset"),
            Self::Io(e) => write!(f, "filesystem error: {}", e),
        }
    }
}

impl From<std::io::Error> for ProvisionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// SHA256 of `bytes`, returned as a 32-byte array.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Decide whether to write a new copy of the DLL.
/// Returns true if the file is missing, unreadable, or its hash differs.
pub fn needs_rewrite(target: &Path, expected_hash: &[u8; 32]) -> bool {
    match std::fs::read(target) {
        Ok(existing) => &sha256(&existing) != expected_hash,
        Err(_) => true,
    }
}

/// Provision the helper DLL into `%LOCALAPPDATA%\Slipkey\` if needed and return
/// its absolute path. Called once at startup, before any TSF dispatch is wired.
pub fn ensure_helper_dll() -> Result<PathBuf, ProvisionError> {
    let dir = local_app_data()?.join("Slipkey");
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(DLL_FILE_NAME);
    let expected = sha256(EMBEDDED_DLL);

    if needs_rewrite(&path, &expected) {
        let tmp = dir.join(format!("{DLL_FILE_NAME}.tmp"));
        std::fs::write(&tmp, EMBEDDED_DLL)?;
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path)?;
        log::info!("provisioned helper DLL: {}", path.display());
    } else {
        log::debug!("helper DLL up to date: {}", path.display());
    }

    Ok(path)
}

fn local_app_data() -> Result<PathBuf, ProvisionError> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(ProvisionError::NoLocalAppData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sha256_of_empty_input_is_known() {
        let h = sha256(b"");
        assert_eq!(
            hex::encode(h),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn needs_rewrite_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.dll");
        assert!(needs_rewrite(&path, &sha256(b"hello")));
    }

    #[test]
    fn needs_rewrite_when_hash_differs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file.dll");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"world").unwrap();
        drop(f);
        assert!(needs_rewrite(&path, &sha256(b"hello")));
    }

    #[test]
    fn no_rewrite_when_hash_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file.dll");
        std::fs::write(&path, b"hello").unwrap();
        assert!(!needs_rewrite(&path, &sha256(b"hello")));
    }
}
