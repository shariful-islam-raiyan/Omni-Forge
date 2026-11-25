use crate::error::{OmniError, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Recursively copies `src` into `dst`, creating `dst` if needed. Used both
/// for cache restore/store and for copying public assets into the output
/// directory. Both paths are expected to already be validated/contained by
/// the caller (e.g. via `OutputDir`).
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| OmniError::Io {
        path: dst.to_path_buf(),
        source: e,
    })?;
    for entry in fs::read_dir(src).map_err(|e| OmniError::Io {
        path: src.to_path_buf(),
        source: e,
    })? {
        let entry = entry.map_err(|e| OmniError::Io {
            path: src.to_path_buf(),
            source: e,
        })?;
        let file_type = entry.file_type().map_err(|e| OmniError::Io {
            path: entry.path(),
            source: e,
        })?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dest_path).map_err(|e| OmniError::Io {
                path: entry.path(),
                source: e,
            })?;
        }
        // symlinks are intentionally skipped: we never want cache/output
        // trees to contain a link that could point outside the sandboxed
        // directory.
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| OmniError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(sha256_hex(&bytes))
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}
