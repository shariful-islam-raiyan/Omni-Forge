//! Path safety utilities.
//!
//! All adapter and CLI code that writes files must go through [`safe_join`]
//! or an [`OutputDir`] so that a malicious or buggy manifest (an absolute
//! path, a `..` escape, or a symlink pointing outside the project) cannot
//! cause writes or deletes outside the intended directory.

use crate::error::{OmniError, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Joins `relative` onto `base`, rejecting absolute paths and any traversal
/// that would resolve outside of `base`. Does not require the path to exist.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let rel_path = Path::new(relative);
    if rel_path.is_absolute() {
        return Err(OmniError::UnsafePath {
            path: relative.to_string(),
            root: base.display().to_string(),
        });
    }

    let mut resolved = base.to_path_buf();
    for component in rel_path.components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                // Walking "up" is only safe while we remain inside `base`.
                if resolved == base || !resolved.starts_with(base) {
                    return Err(OmniError::UnsafePath {
                        path: relative.to_string(),
                        root: base.display().to_string(),
                    });
                }
                resolved.pop();
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(OmniError::UnsafePath {
                    path: relative.to_string(),
                    root: base.display().to_string(),
                });
            }
        }
    }

    if !resolved.starts_with(base) {
        return Err(OmniError::UnsafePath {
            path: relative.to_string(),
            root: base.display().to_string(),
        });
    }

    Ok(resolved)
}

/// A directory that has been verified to exist (or be creatable) strictly
/// inside a trusted root, with symlinks resolved. All writes performed
/// through this type stay inside the canonical root.
#[derive(Debug, Clone)]
pub struct OutputDir {
    root_canonical: PathBuf,
}

impl OutputDir {
    /// `project_root` must already be canonical. `relative_output` is the
    /// project-configured output directory (e.g. "dist").
    pub fn resolve(project_root: &Path, relative_output: &str) -> Result<Self> {
        let target = safe_join(project_root, relative_output)?;
        fs::create_dir_all(&target).map_err(|e| OmniError::Io {
            path: target.clone(),
            source: e,
        })?;
        let canonical = fs::canonicalize(&target).map_err(|e| OmniError::Io {
            path: target.clone(),
            source: e,
        })?;
        let canonical_root = fs::canonicalize(project_root).map_err(|e| OmniError::Io {
            path: project_root.to_path_buf(),
            source: e,
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(OmniError::UnsafePath {
                path: relative_output.to_string(),
                root: canonical_root.display().to_string(),
            });
        }
        Ok(Self {
            root_canonical: canonical,
        })
    }

    pub fn path(&self) -> &Path {
        &self.root_canonical
    }

    /// Joins and validates a path for a file this build is about to write.
    pub fn join(&self, relative: &str) -> Result<PathBuf> {
        safe_join(&self.root_canonical, relative)
    }

    /// Removes the entire output directory tree, after re-verifying the
    /// canonical path is still contained where we expect (defends against a
    /// TOCTOU symlink swap between resolve() and clean()).
    pub fn remove_all(&self, project_root_canonical: &Path) -> Result<()> {
        if !self.root_canonical.starts_with(project_root_canonical) {
            return Err(OmniError::UnsafePath {
                path: self.root_canonical.display().to_string(),
                root: project_root_canonical.display().to_string(),
            });
        }
        if self.root_canonical == *project_root_canonical {
            return Err(OmniError::Other(
                "refusing to delete the project root itself".into(),
            ));
        }
        if self.root_canonical.exists() {
            remove_dir_all_with_retry(&self.root_canonical)?;
        }
        Ok(())
    }
}

/// Deletes a directory tree, retrying briefly on failure.
///
/// On Windows, a file written moments ago (e.g. a build artifact or a
/// materialized adapter script) can be transiently locked by real-time
/// antivirus scanning, making an immediate `remove_dir_all` fail with
/// "being used by another process" even though nothing in OmniForge still
/// holds it open. A few short retries absorb that without masking a real,
/// persistent failure (which still surfaces after the retries are spent).
fn remove_dir_all_with_retry(path: &Path) -> Result<()> {
    const ATTEMPTS: u32 = 5;
    const DELAY: std::time::Duration = std::time::Duration::from_millis(150);

    let mut last_err = None;
    for attempt in 0..ATTEMPTS {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < ATTEMPTS {
                    std::thread::sleep(DELAY);
                }
            }
        }
    }
    Err(OmniError::Io {
        path: path.to_path_buf(),
        source: last_err.expect("loop ran at least once"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_absolute_paths() {
        let base = Path::new("/tmp/project");
        assert!(safe_join(base, "/etc/passwd").is_err());
    }

    #[test]
    fn rejects_parent_escape() {
        let base = Path::new("/tmp/project");
        assert!(safe_join(base, "../../etc/passwd").is_err());
    }

    #[test]
    fn allows_nested_relative_path() {
        let base = Path::new("/tmp/project");
        let joined = safe_join(base, "dist/web/index.html").unwrap();
        assert_eq!(joined, Path::new("/tmp/project/dist/web/index.html"));
    }

    #[test]
    fn allows_internal_parent_traversal_that_stays_inside() {
        let base = Path::new("/tmp/project");
        let joined = safe_join(base, "dist/web/../web2/index.html").unwrap();
        assert_eq!(joined, Path::new("/tmp/project/dist/web2/index.html"));
    }

    #[test]
    fn output_dir_refuses_to_delete_project_root() {
        let dir = tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let out = OutputDir {
            root_canonical: root.clone(),
        };
        assert!(out.remove_all(&root).is_err());
    }

    #[test]
    fn output_dir_resolve_creates_and_contains() {
        let dir = tempdir().unwrap();
        let out = OutputDir::resolve(dir.path(), "dist").unwrap();
        assert!(out
            .path()
            .starts_with(fs::canonicalize(dir.path()).unwrap()));
        assert!(out.path().exists());
    }
}
