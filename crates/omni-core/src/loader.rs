//! Loads and validates `omni.yaml` into a [`ProjectFile`], plus the shared
//! "find the project root" logic the CLI uses.

use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::error::{OmniError, Result};
use crate::schema::{self, SchemaKind};
use crate::spec::project::{ProjectFile, KNOWN_TARGETS, PROJECT_SCHEMA_VERSION};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE_NAME: &str = "omni.yaml";

/// Walks upward from `start` looking for `omni.yaml`. Returns the directory
/// containing it (the project root), not the file itself.
pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut dir = fs::canonicalize(start).map_err(|e| OmniError::Io {
        path: start.to_path_buf(),
        source: e,
    })?;
    loop {
        if dir.join(PROJECT_FILE_NAME).is_file() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => {
                return Err(OmniError::Diagnostic(
                    Diagnostic::error(
                        "E-PROJECT-NOT-FOUND",
                        format!("no '{PROJECT_FILE_NAME}' found in '{}' or any parent directory", start.display()),
                    )
                    .with_suggestion("run 'omni create <name>' to scaffold a new project, or cd into an existing one"),
                ));
            }
        }
    }
}

#[derive(Debug)]
pub struct LoadedProject {
    pub file: ProjectFile,
    pub root: PathBuf,
}

/// Loads, schema-validates, and semantically validates the project file at
/// `root/omni.yaml`. Returns all diagnostics found; the caller decides
/// whether warnings are acceptable, but any error diagnostic means `file`
/// should not be trusted for a build.
pub fn load_project(root: &Path) -> Result<(LoadedProject, Diagnostics)> {
    let mut diags = Diagnostics::new();
    let path = root.join(PROJECT_FILE_NAME);
    let raw = fs::read_to_string(&path).map_err(|e| OmniError::Io {
        path: path.clone(),
        source: e,
    })?;

    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|e| {
        OmniError::Diagnostic(
            Diagnostic::error("E-PROJECT-YAML", format!("invalid YAML: {e}"))
                .with_file(path.display().to_string()),
        )
    })?;

    // Validate against the JSON Schema via a JSON round-trip (YAML is a
    // superset of JSON for our purposes; this keeps one schema definition
    // for both machine protocol messages and human-edited config).
    let json_value: serde_json::Value = serde_json::to_value(&yaml_value).map_err(|e| {
        OmniError::Diagnostic(
            Diagnostic::error("E-PROJECT-YAML", e.to_string())
                .with_file(path.display().to_string()),
        )
    })?;

    let schema_errors = schema::validate(SchemaKind::Project, &json_value, PROJECT_FILE_NAME);
    if !schema_errors.is_empty() {
        for d in &schema_errors {
            diags.push(d.clone());
        }
        return Err(OmniError::Invalid(schema_errors));
    }

    let file: ProjectFile = serde_yaml::from_value(yaml_value).map_err(|e| {
        OmniError::Diagnostic(
            Diagnostic::error(
                "E-PROJECT-PARSE",
                format!("failed to parse project file: {e}"),
            )
            .with_file(path.display().to_string()),
        )
    })?;

    if file.schema != PROJECT_SCHEMA_VERSION {
        return Err(OmniError::Diagnostic(
            Diagnostic::error(
                "E-PROJECT-SCHEMA-VERSION",
                format!(
                    "unsupported schema version '{}': this build of OmniForge supports '{}'",
                    file.schema, PROJECT_SCHEMA_VERSION
                ),
            )
            .with_file(path.display().to_string())
            .with_suggestion(
                "upgrade OmniForge, or migrate the project file to the supported schema version",
            ),
        ));
    }

    semantic_checks(&file, &path, &mut diags);

    if diags.has_errors() {
        return Err(OmniError::Invalid(diags.into_items()));
    }

    Ok((
        LoadedProject {
            file,
            root: root.to_path_buf(),
        },
        diags,
    ))
}

fn semantic_checks(file: &ProjectFile, path: &Path, diags: &mut Diagnostics) {
    for (name, cfg) in &file.targets {
        if !KNOWN_TARGETS.contains(&name.as_str()) {
            diags.push(
                Diagnostic::error(
                    "E-PROJECT-UNKNOWN-TARGET",
                    format!("unknown target '{name}'"),
                )
                .with_file(path.display().to_string())
                .with_target(name.clone())
                .with_suggestion(format!("known targets: {}", KNOWN_TARGETS.join(", "))),
            );
            continue;
        }
        if cfg.enabled && cfg.adapter.is_none() {
            diags.push(
                Diagnostic::error(
                    "E-PROJECT-MISSING-ADAPTER",
                    format!("target '{name}' is enabled but does not declare an 'adapter'"),
                )
                .with_file(path.display().to_string())
                .with_target(name.clone()),
            );
        }
    }

    if file.project.name.is_empty() {
        diags.push(
            Diagnostic::error("E-PROJECT-NAME", "project name must not be empty")
                .with_file(path.display().to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_project(dir: &Path, contents: &str) {
        fs::write(dir.join(PROJECT_FILE_NAME), contents).unwrap();
    }

    #[test]
    fn loads_valid_minimal_project() {
        let dir = tempdir().unwrap();
        write_project(
            dir.path(),
            r#"
schema: omniforge.dev/project/v1
project:
  name: hello-web
  version: 0.1.0
targets:
  web:
    enabled: true
    adapter: web-static
    entry: web/app.yaml
"#,
        );
        let (loaded, diags) = load_project(dir.path()).unwrap();
        assert!(!diags.has_errors());
        assert_eq!(loaded.file.project.name, "hello-web");
        assert_eq!(loaded.file.build.output, "dist");
    }

    #[test]
    fn rejects_unknown_target_name() {
        let dir = tempdir().unwrap();
        write_project(
            dir.path(),
            r#"
schema: omniforge.dev/project/v1
project:
  name: hello-web
  version: 0.1.0
targets:
  quantum:
    enabled: true
"#,
        );
        let err = load_project(dir.path()).unwrap_err();
        match err {
            OmniError::Invalid(items) => {
                assert!(items.iter().any(|d| d.code == "E-PROJECT-UNKNOWN-TARGET"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn rejects_enabled_target_without_adapter() {
        let dir = tempdir().unwrap();
        write_project(
            dir.path(),
            r#"
schema: omniforge.dev/project/v1
project:
  name: hello-web
  version: 0.1.0
targets:
  web:
    enabled: true
"#,
        );
        let err = load_project(dir.path()).unwrap_err();
        match err {
            OmniError::Invalid(items) => {
                assert!(items.iter().any(|d| d.code == "E-PROJECT-MISSING-ADAPTER"));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let dir = tempdir().unwrap();
        write_project(
            dir.path(),
            r#"
schema: omniforge.dev/project/v2
project:
  name: hello-web
  version: 0.1.0
targets:
  web:
    enabled: true
    adapter: web-static
"#,
        );
        assert!(load_project(dir.path()).is_err());
    }

    #[test]
    fn find_project_root_walks_up_parents() {
        let dir = tempdir().unwrap();
        write_project(
            dir.path(),
            "schema: omniforge.dev/project/v1\nproject:\n  name: x\n  version: 0.1.0\ntargets:\n  web:\n    enabled: false\n",
        );
        let nested = dir.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        let root = find_project_root(&nested).unwrap();
        assert_eq!(root, fs::canonicalize(dir.path()).unwrap());
    }

    #[test]
    fn find_project_root_errors_when_missing() {
        let dir = tempdir().unwrap();
        assert!(find_project_root(dir.path()).is_err());
    }
}
