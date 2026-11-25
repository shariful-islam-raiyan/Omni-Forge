//! Component manifest discovery and validation.

use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::error::{OmniError, Result};
use crate::paths::safe_join;
use crate::schema::{self, SchemaKind};
use crate::spec::component::{ComponentManifest, COMPONENT_SCHEMA_VERSION};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const COMPONENT_FILE_NAME: &str = "component.yaml";

/// Discovers every `component.yaml` directly under immediate subdirectories
/// of `components_dir_rel` (resolved safely against `project_root`),
/// validates each against the schema, and checks for duplicate names.
///
/// Returns the manifests that parsed successfully (possibly with warnings)
/// plus the full diagnostics list; a manifest with an associated error
/// diagnostic is excluded from the returned list.
pub fn discover_components(
    project_root: &Path,
    components_dir_rel: &str,
) -> Result<(Vec<ComponentManifest>, Diagnostics)> {
    let mut diags = Diagnostics::new();
    let components_dir = safe_join(project_root, components_dir_rel)?;

    if !components_dir.exists() {
        diags.push(Diagnostic::warning(
            "W-COMPONENT-DIR-MISSING",
            format!(
                "components directory '{}' does not exist; no components will be available",
                components_dir_rel
            ),
        ));
        return Ok((Vec::new(), diags));
    }

    let mut manifests = Vec::new();
    let mut seen_names: HashMap<String, std::path::PathBuf> = HashMap::new();

    let mut entries: Vec<_> = fs::read_dir(&components_dir)
        .map_err(|e| OmniError::Io {
            path: components_dir.clone(),
            source: e,
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let dir = entry.path();
        let manifest_path = dir.join(COMPONENT_FILE_NAME);
        if !manifest_path.is_file() {
            continue;
        }

        let raw = match fs::read_to_string(&manifest_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-COMPONENT-IO", format!("could not read manifest: {e}"))
                        .with_file(manifest_path.display().to_string()),
                );
                continue;
            }
        };

        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-COMPONENT-YAML", format!("invalid YAML: {e}"))
                        .with_file(manifest_path.display().to_string()),
                );
                continue;
            }
        };

        let json_value: serde_json::Value = match serde_json::to_value(&yaml_value) {
            Ok(v) => v,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-COMPONENT-YAML", e.to_string())
                        .with_file(manifest_path.display().to_string()),
                );
                continue;
            }
        };

        let schema_errors = schema::validate(
            SchemaKind::Component,
            &json_value,
            &manifest_path.display().to_string(),
        );
        if !schema_errors.is_empty() {
            for d in schema_errors {
                diags.push(d);
            }
            continue;
        }

        let mut manifest: ComponentManifest = match serde_yaml::from_value(yaml_value) {
            Ok(m) => m,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-COMPONENT-PARSE",
                        format!("failed to parse manifest: {e}"),
                    )
                    .with_file(manifest_path.display().to_string()),
                );
                continue;
            }
        };

        if manifest.schema != COMPONENT_SCHEMA_VERSION {
            diags.push(
                Diagnostic::error(
                    "E-COMPONENT-SCHEMA-VERSION",
                    format!(
                        "unsupported component schema version '{}': supported is '{}'",
                        manifest.schema, COMPONENT_SCHEMA_VERSION
                    ),
                )
                .with_file(manifest_path.display().to_string())
                .with_component(manifest.name.clone()),
            );
            continue;
        }

        // Every implementation `source` must resolve inside the project.
        let mut source_invalid = false;
        for (target, implementation) in &manifest.implementations {
            if safe_join(&dir, &implementation.source).is_err() {
                diags.push(
                    Diagnostic::error(
                        "E-COMPONENT-UNSAFE-SOURCE",
                        format!(
                            "implementation source '{}' for target '{target}' escapes the project directory",
                            implementation.source
                        ),
                    )
                    .with_file(manifest_path.display().to_string())
                    .with_component(manifest.name.clone())
                    .with_target(target.clone()),
                );
                source_invalid = true;
            }
        }
        if source_invalid {
            continue;
        }

        if let Some(existing) = seen_names.get(&manifest.name) {
            diags.push(
                Diagnostic::error(
                    "E-COMPONENT-DUPLICATE",
                    format!(
                        "component '{}' is defined more than once ('{}' and '{}')",
                        manifest.name,
                        existing.display(),
                        manifest_path.display()
                    ),
                )
                .with_file(manifest_path.display().to_string())
                .with_component(manifest.name.clone()),
            );
            continue;
        }
        seen_names.insert(manifest.name.clone(), manifest_path.clone());

        manifest.manifest_path = manifest_path.clone();
        manifest.manifest_dir = dir.clone();
        manifests.push(manifest);
    }

    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((manifests, diags))
}
