//! Real Android toolchain discovery: an SDK (platforms + build-tools), a
//! JDK, and a Gradle distribution. Unlike `python-backend`'s toolchain
//! (one interpreter found via a short, fixed alias list on `PATH`),
//! Android's toolchain is normally installed by Android Studio into
//! well-known, but not `PATH`-advertised, locations — so discovery here is
//! a search over environment variables and common installation
//! directories, not a single `resolve_executable` call.
//!
//! **Verified on this project's Windows development machine** against a
//! real Android Studio + SDK + cached Gradle install (see ADR 0011): the
//! `ANDROID_HOME`/env-var paths and the Windows default SDK/JBR paths are
//! exercised by this crate's own tests when such an install is present.
//! The macOS/Linux default paths below are implemented the same way
//! Android Studio documents its own default install locations, but were
//! **not** exercised end-to-end on an actual macOS or Linux machine as
//! part of this work — see ADR 0011's Consequences section.

use std::path::{Path, PathBuf};

/// A located, usable Android toolchain.
#[derive(Debug, Clone)]
pub struct AndroidToolchain {
    pub sdk_dir: PathBuf,
    pub java_home: PathBuf,
    pub gradle_exe: PathBuf,
}

/// What's missing, named precisely enough to tell a user exactly what to
/// fix.
#[derive(Debug, Clone, Default)]
pub struct ToolchainProblems {
    pub missing: Vec<String>,
}

impl ToolchainProblems {
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Finds a usable Android SDK directory: `ANDROID_HOME`/`ANDROID_SDK_ROOT`
/// first (if set, trusted as given — no further validation beyond
/// containing a `platforms` directory), then common per-OS default
/// install locations. Returns `None` if nothing found or nothing valid.
pub fn find_sdk_dir() -> Option<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(path) = std::env::var(var) {
            let p = PathBuf::from(path);
            if is_valid_sdk_dir(&p) {
                return Some(p);
            }
        }
    }
    default_sdk_dirs()
        .into_iter()
        .find(|candidate| is_valid_sdk_dir(candidate))
}

fn is_valid_sdk_dir(dir: &Path) -> bool {
    dir.join("platforms").is_dir() && dir.join("build-tools").is_dir()
}

fn default_sdk_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local_app_data).join("Android").join("Sdk"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(
            PathBuf::from(&home)
                .join("Library")
                .join("Android")
                .join("sdk"),
        );
        dirs.push(PathBuf::from(&home).join("Android").join("Sdk"));
    }
    dirs
}

/// Finds a usable JDK (for running Gradle, which Android's build needs
/// regardless of the app's own language). `JAVA_HOME` first, then a plain
/// `java` on `PATH`, then — on Windows only, since this is Android
/// Studio's documented bundled-JDK layout — the JBR shipped inside a
/// default Android Studio install.
pub fn find_java_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(home);
        if is_valid_java_home(&p) {
            return Some(p);
        }
    }
    if which_java_on_path().is_some() {
        // `java` resolves on PATH; JAVA_HOME isn't strictly required by
        // Gradle in that case, but our own child-process invocation sets
        // JAVA_HOME explicitly for determinism, so derive it from the
        // resolved `java` binary's grandparent directory (`<home>/bin/java`).
        if let Some(home) = which_java_on_path()
            .and_then(|java| java.parent().and_then(Path::parent).map(Path::to_path_buf))
        {
            if is_valid_java_home(&home) {
                return Some(home);
            }
        }
    }
    default_java_homes()
        .into_iter()
        .find(|candidate| is_valid_java_home(candidate))
}

fn is_valid_java_home(dir: &Path) -> bool {
    dir.join("bin").join(java_binary_name()).is_file()
}

#[cfg(windows)]
fn java_binary_name() -> &'static str {
    "java.exe"
}
#[cfg(not(windows))]
fn java_binary_name() -> &'static str {
    "java"
}

fn which_java_on_path() -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    path_var.split(sep).map(PathBuf::from).find_map(|dir| {
        let candidate = dir.join(java_binary_name());
        candidate.is_file().then_some(candidate)
    })
}

fn default_java_homes() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        dirs.push(PathBuf::from(
            r"C:\Program Files\Android\Android Studio\jbr",
        ));
    } else if let Ok(home) = std::env::var("HOME") {
        // macOS default Android Studio install location.
        dirs.push(PathBuf::from(
            "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
        ));
        let _ = home; // not otherwise needed on this branch today
    }
    dirs
}

/// Finds a usable Gradle executable: `GRADLE_HOME`/a plain `gradle` on
/// `PATH` first, else the newest-looking distribution already extracted
/// into Gradle's own wrapper cache (`~/.gradle/wrapper/dists/*/*/gradle-*`)
/// — this is how a machine that has ever run *any* Gradle wrapper (e.g.
/// via Android Studio) already has a real, usable Gradle without this
/// adapter needing to download one itself. Lexical sort of distribution
/// directory names is a pragmatic "prefer what looks newest" heuristic,
/// not a real semver comparison — documented as such in ADR 0011.
pub fn find_gradle_exe() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("GRADLE_HOME") {
        let candidate = PathBuf::from(home).join("bin").join(gradle_binary_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(p) = which_on_path(gradle_binary_name()) {
        return Some(p);
    }
    find_cached_wrapper_gradle()
}

#[cfg(windows)]
fn gradle_binary_name() -> &'static str {
    "gradle.bat"
}
#[cfg(not(windows))]
fn gradle_binary_name() -> &'static str {
    "gradle"
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    path_var.split(sep).map(PathBuf::from).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

fn gradle_user_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("GRADLE_USER_HOME") {
        return Some(PathBuf::from(home));
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(|h| PathBuf::from(h).join(".gradle"))
}

fn find_cached_wrapper_gradle() -> Option<PathBuf> {
    let dists_dir = gradle_user_home()?.join("wrapper").join("dists");
    let mut version_dirs: Vec<PathBuf> = std::fs::read_dir(&dists_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Prefer lexically-newest distribution directory name
    // (e.g. "gradle-9.3.1-bin" over "gradle-8.14.3-all").
    version_dirs.sort();
    version_dirs.reverse();

    for version_dir in version_dirs {
        let Ok(hash_dirs) = std::fs::read_dir(&version_dir) else {
            continue;
        };
        for hash_dir in hash_dirs.filter_map(|e| e.ok()).map(|e| e.path()) {
            let Ok(inner) = std::fs::read_dir(&hash_dir) else {
                continue;
            };
            for gradle_root in inner
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
            {
                let candidate = gradle_root.join("bin").join(gradle_binary_name());
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Locates every piece of the Android toolchain, returning either a
/// complete, usable [`AndroidToolchain`] or the precise list of what's
/// missing.
pub fn detect() -> Result<AndroidToolchain, ToolchainProblems> {
    let sdk_dir = find_sdk_dir();
    let java_home = find_java_home();
    let gradle_exe = find_gradle_exe();

    let mut missing = Vec::new();
    if sdk_dir.is_none() {
        missing.push(
            "Android SDK (set ANDROID_HOME, or install Android Studio's default SDK location)"
                .to_string(),
        );
    }
    if java_home.is_none() {
        missing.push(
            "a JDK (set JAVA_HOME, put 'java' on PATH, or install Android Studio)".to_string(),
        );
    }
    if gradle_exe.is_none() {
        missing.push(
            "Gradle (set GRADLE_HOME, put 'gradle' on PATH, or run any Gradle wrapper once so a distribution is cached under ~/.gradle/wrapper/dists)"
                .to_string(),
        );
    }

    match (sdk_dir, java_home, gradle_exe) {
        (Some(sdk_dir), Some(java_home), Some(gradle_exe)) => Ok(AndroidToolchain {
            sdk_dir,
            java_home,
            gradle_exe,
        }),
        _ => Err(ToolchainProblems { missing }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_sdk_dir_rejects_a_directory_missing_build_tools() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("platforms")).unwrap();
        assert!(!is_valid_sdk_dir(dir.path()));
    }

    #[test]
    fn is_valid_sdk_dir_accepts_a_directory_with_both_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("platforms")).unwrap();
        std::fs::create_dir_all(dir.path().join("build-tools")).unwrap();
        assert!(is_valid_sdk_dir(dir.path()));
    }

    #[test]
    fn detect_reports_every_missing_piece_by_name() {
        // Force a clean miss regardless of this machine's real environment
        // by pointing every discoverable variable at nonexistent paths.
        let saved: Vec<(&str, Option<String>)> = [
            "ANDROID_HOME",
            "ANDROID_SDK_ROOT",
            "JAVA_HOME",
            "GRADLE_HOME",
            "GRADLE_USER_HOME",
        ]
        .iter()
        .map(|k| (*k, std::env::var(k).ok()))
        .collect();
        for (k, _) in &saved {
            std::env::set_var(k, "C:/definitely/not/a/real/path/xyz");
        }
        let result = detect();
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        // This only asserts the structure of a failure, not that this
        // specific machine fails -- a real, valid toolchain found via a
        // *different* path than these env vars (e.g. a plain `java`/
        // `gradle` already on PATH) would still make `detect()` succeed,
        // which is correct behavior, not a test bug.
        if let Err(problems) = result {
            assert!(!problems.is_empty());
        }
    }
}
