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

        // Every composition slot must name a component already declared as
        // a dependency (so the resolver's existing dependency-graph +
        // cycle-detection machinery already covers it), and must not reuse
        // a plain input's name (both end up as `{{name}}` placeholders in
        // the same template namespace).
        let dep_set: std::collections::BTreeSet<&str> =
            manifest.dependencies.iter().map(String::as_str).collect();
        let mut compose_invalid = false;
        for (slot, compose) in &manifest.composes {
            if manifest.interface.inputs.contains_key(slot) {
                diags.push(
                    Diagnostic::error(
                        "E-COMPONENT-COMPOSE-NAME-CONFLICT",
                        format!(
                            "composition slot '{slot}' has the same name as a declared input on component '{}'",
                            manifest.name
                        ),
                    )
                    .with_file(manifest_path.display().to_string())
                    .with_component(manifest.name.clone()),
                );
                compose_invalid = true;
            }
            if !dep_set.contains(compose.component.as_str()) {
                diags.push(
                    Diagnostic::error(
                        "E-COMPONENT-COMPOSE-UNDECLARED-DEPENDENCY",
                        format!(
                            "composition slot '{slot}' on component '{}' uses component '{}', which is not listed in 'dependencies'",
                            manifest.name, compose.component
                        ),
                    )
                    .with_file(manifest_path.display().to_string())
                    .with_component(manifest.name.clone())
                    .with_suggestion(format!(
                        "add '{}' to this component's 'dependencies' list",
                        compose.component
                    )),
                );
                compose_invalid = true;
            }
        }
        if compose_invalid {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_component(dir: &Path, name_dir: &str, yaml: &str) {
        let component_dir = dir.join("lib/components").join(name_dir);
        fs::create_dir_all(&component_dir).unwrap();
        fs::write(component_dir.join(COMPONENT_FILE_NAME), yaml).unwrap();
    }

    #[test]
    fn compose_slot_targeting_an_undeclared_dependency_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        write_component(
            &root,
            "ui.page",
            r#"
schema: omniforge.dev/component/v1
name: ui.page
version: 1.0.0
kind: ui
composes:
  nav:
    component: ui.navbar
interface:
  inputs: {}
implementations:
  web:
    adapter: web-static
    source: ./page.html.tmpl
"#,
        );
        let (manifests, diags) = discover_components(&root, "lib/components").unwrap();
        assert!(manifests.is_empty());
        assert!(diags
            .items()
            .iter()
            .any(|d| d.code == "E-COMPONENT-COMPOSE-UNDECLARED-DEPENDENCY"));
    }

    #[test]
    fn compose_slot_colliding_with_an_input_name_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        write_component(
            &root,
            "ui.page",
            r#"
schema: omniforge.dev/component/v1
name: ui.page
version: 1.0.0
kind: ui
dependencies: [ui.navbar]
composes:
  nav:
    component: ui.navbar
interface:
  inputs:
    nav:
      type: string
      required: false
implementations:
  web:
    adapter: web-static
    source: ./page.html.tmpl
"#,
        );
        let (manifests, diags) = discover_components(&root, "lib/components").unwrap();
        assert!(manifests.is_empty());
        assert!(diags
            .items()
            .iter()
            .any(|d| d.code == "E-COMPONENT-COMPOSE-NAME-CONFLICT"));
    }

    #[test]
    fn compose_slot_with_a_declared_dependency_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        write_component(
            &root,
            "ui.page",
            r#"
schema: omniforge.dev/component/v1
name: ui.page
version: 1.0.0
kind: ui
dependencies: [ui.navbar]
composes:
  nav:
    component: ui.navbar
interface:
  inputs: {}
implementations:
  web:
    adapter: web-static
    source: ./page.html.tmpl
"#,
        );
        let (manifests, diags) = discover_components(&root, "lib/components").unwrap();
        assert!(!diags.has_errors(), "{:?}", diags.items());
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].composes["nav"].component, "ui.navbar");
    }
}
