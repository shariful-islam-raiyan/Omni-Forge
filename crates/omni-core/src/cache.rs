//! A minimal, correct, content-addressed local build cache.
//!
//! Scope (see docs/caching.md): keyed by a SHA-256 hash over every input
//! that affects a target's output (adapter identity+version, component
//! manifests, template/source content, public asset content, and relevant
//! build config) — never by file modification times, and never including
//! secret values. A cache hit restores the previously produced output tree
//! verbatim; a miss means the adapter must run and its output is then
//! stored under the computed key.

use crate::error::Result;
use crate::fsutil::{copy_dir_all, sha256_hex};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct BuildCache {
    root: PathBuf,
}

/// Accumulates cache-key inputs in a stable order and hashes them together.
/// Each `add_*` call feeds a length-prefixed chunk so inputs can't be
/// confused with each other by concatenation (e.g. "ab"+"c" vs "a"+"bc").
#[derive(Default)]
pub struct CacheKeyBuilder {
    hasher: Sha256,
}

impl CacheKeyBuilder {
    pub fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    pub fn add(&mut self, label: &str, bytes: &[u8]) -> &mut Self {
        self.hasher.update(label.as_bytes());
        self.hasher.update((bytes.len() as u64).to_le_bytes());
        self.hasher.update(bytes);
        self
    }

    pub fn add_str(&mut self, label: &str, value: &str) -> &mut Self {
        self.add(label, value.as_bytes())
    }

    pub fn finish(self) -> String {
        sha256_hex(&self.hasher.finalize())
    }
}

impl BuildCache {
    pub fn open(project_root: &Path) -> Result<Self> {
        let root = project_root.join(".omniforge").join("cache");
        std::fs::create_dir_all(&root).map_err(|e| crate::error::OmniError::Io {
            path: root.clone(),
            source: e,
        })?;
        Ok(Self { root })
    }

    fn entry_dir(&self, target: &str, key: &str) -> PathBuf {
        self.root.join(target).join(key)
    }

    pub fn lookup(&self, target: &str, key: &str) -> Option<PathBuf> {
        let dir = self.entry_dir(target, key);
        if dir.is_dir() {
            Some(dir)
        } else {
            None
        }
    }

    /// Restores a cached output tree into `dest` (which must already be a
    /// validated, contained output directory).
    pub fn restore(&self, target: &str, key: &str, dest: &Path) -> Result<bool> {
        match self.lookup(target, key) {
            Some(cached) => {
                copy_dir_all(&cached, dest)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Stores `source` (an already-produced output tree) under `key`.
    pub fn store(&self, target: &str, key: &str, source: &Path) -> Result<()> {
        let dir = self.entry_dir(target, key);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| crate::error::OmniError::Io {
                path: dir.clone(),
                source: e,
            })?;
        }
        copy_dir_all(source, &dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn same_inputs_produce_same_key() {
        let mut a = CacheKeyBuilder::new();
        a.add_str("x", "hello").add_str("y", "world");
        let mut b = CacheKeyBuilder::new();
        b.add_str("x", "hello").add_str("y", "world");
        assert_eq!(a.finish(), b.finish());
    }

    #[test]
    fn different_inputs_produce_different_keys() {
        let mut a = CacheKeyBuilder::new();
        a.add_str("x", "hello");
        let mut b = CacheKeyBuilder::new();
        b.add_str("x", "hello!");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn concatenation_ambiguity_is_avoided_by_length_prefixing() {
        let mut a = CacheKeyBuilder::new();
        a.add_str("k", "ab").add_str("k", "c");
        let mut b = CacheKeyBuilder::new();
        b.add_str("k", "a").add_str("k", "bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn store_then_restore_roundtrips_files() {
        let dir = tempdir().unwrap();
        let cache = BuildCache::open(dir.path()).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("index.html"), b"<h1>hi</h1>").unwrap();

        cache.store("web", "abc123", &src).unwrap();
        assert!(cache.lookup("web", "abc123").is_some());

        let dest = dir.path().join("dest");
        let restored = cache.restore("web", "abc123", &dest).unwrap();
        assert!(restored);
        assert_eq!(
            std::fs::read_to_string(dest.join("index.html")).unwrap(),
            "<h1>hi</h1>"
        );
    }

    #[test]
    fn miss_when_key_not_present() {
        let dir = tempdir().unwrap();
        let cache = BuildCache::open(dir.path()).unwrap();
        let dest = dir.path().join("dest");
        let restored = cache.restore("web", "nope", &dest).unwrap();
        assert!(!restored);
    }
}
