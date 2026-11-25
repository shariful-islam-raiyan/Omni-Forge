//! `omni package <target|all>`: archives an already-built target's output
//! directory (`dist/<target>/`) into a single distributable `.zip` file
//! (`dist/<target>.zip`).
//!
//! Deliberately **not** a new adapter-protocol operation and **not** tied
//! to any adapter: packaging a flat output directory into one archive is
//! the same operation regardless of which adapter produced it, so this is
//! implemented once, generically, at the CLI level. This also corrects a
//! pre-existing gap: `adapters/web-static/adapter.yaml` used to declare
//! `package` as a supported *capability* despite the `Adapter` trait never
//! having a `package` method and nothing ever dispatching it — that false
//! claim has been removed from the manifest; this command is the real,
//! working "package" functionality `omni` actually provides.
//!
//! Requires the target to already be built (`omni build <target>` first)
//! rather than triggering a build itself — packaging and building are
//! kept as separate, explicit steps.

use omni_core::loader;
use omni_core::paths::safe_join;
use omni_core::spec::project::KNOWN_TARGETS;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn run(cwd: &Path, target_arg: &str) -> i32 {
    let root = match loader::find_project_root(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let (loaded, _diags) = match loader::load_project(&root) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let target_names: Vec<String> = if target_arg == "all" {
        loaded
            .file
            .targets
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        if !KNOWN_TARGETS.contains(&target_arg) {
            eprintln!(
                "error: E-PACKAGE-UNKNOWN-TARGET: '{target_arg}' is not a known OmniForge target.\n  known targets: {}",
                KNOWN_TARGETS.join(", ")
            );
            return 1;
        }
        vec![target_arg.to_string()]
    };

    if target_names.is_empty() {
        println!("No enabled targets to package.");
        return 0;
    }

    let mut any_failed = false;
    for target in &target_names {
        match package_one(&root, &loaded.file.build.output, target) {
            Ok(zip_path) => println!("packaged target '{target}' -> {}", zip_path.display()),
            Err(e) => {
                eprintln!("error: {e}");
                any_failed = true;
            }
        }
    }

    if any_failed {
        1
    } else {
        0
    }
}

fn package_one(root: &Path, output_rel: &str, target: &str) -> Result<PathBuf, String> {
    let target_dir = safe_join(root, &format!("{output_rel}/{target}"))
        .map_err(|e| format!("E-PACKAGE-UNSAFE-PATH: {e}"))?;

    if !target_dir.is_dir() {
        return Err(format!(
            "E-PACKAGE-NOT-BUILT: target '{target}' has not been built (no '{}' directory).\n  suggestion: run 'omni build {target}' first",
            target_dir.display()
        ));
    }

    let zip_path = safe_join(root, &format!("{output_rel}/{target}.zip"))
        .map_err(|e| format!("E-PACKAGE-UNSAFE-PATH: {e}"))?;

    let file = fs::File::create(&zip_path).map_err(|e| {
        format!(
            "E-PACKAGE-IO: could not create '{}': {e}",
            zip_path.display()
        )
    })?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    entries.sort();

    for path in entries {
        let relative = path
            .strip_prefix(&target_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        writer
            .start_file(&relative, options)
            .map_err(|e| format!("E-PACKAGE-IO: could not add '{relative}' to archive: {e}"))?;
        let bytes = fs::read(&path)
            .map_err(|e| format!("E-PACKAGE-IO: could not read '{}': {e}", path.display()))?;
        writer
            .write_all(&bytes)
            .map_err(|e| format!("E-PACKAGE-IO: could not write '{relative}' into archive: {e}"))?;
    }

    writer
        .finish()
        .map_err(|e| format!("E-PACKAGE-IO: could not finalize archive: {e}"))?;

    Ok(zip_path)
}
