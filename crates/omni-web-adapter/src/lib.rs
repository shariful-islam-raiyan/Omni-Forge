//! The `web-static` adapter.
//!
//! Scope (see docs/architecture.md and docs/roadmap.md for what is
//! deliberately **not** here): takes one project, renders one root UI
//! component into one `index.html`, copies declared CSS/JS/asset files
//! verbatim, and writes a build manifest. There is no routing, no
//! server-side data fetching, no hydration, and no JavaScript execution
//! engine. Component rendering is a fixed `{{field}}` substitution template
//! (see `template.rs`), not a general-purpose templating or scripting
//! language — the one exception is `ComponentManifest::composes`: a
//! component may declare named slots whose value is *another* component,
//! fully rendered through this same recursive, fixed mechanism (see
//! `render_component` below and
//! `docs/decisions/0010-component-composition-via-slots.md`). That is
//! composition, not a template language growing conditionals or loops.

mod manifest;
mod template;

pub use manifest::{PublicAssets, RootRef, WebAppManifest};

use omni_adapter_api::{Adapter, Artifact, BuildContext, CompileOutput};
use omni_core::diagnostics::{Diagnostic, Diagnostics};
use omni_core::spec::adapter::AdapterManifest;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::OnceLock;

const ADAPTER_MANIFEST_SRC: &str = include_str!("../../../adapters/web-static/adapter.yaml");

pub struct WebStaticAdapter {
    manifest: AdapterManifest,
}

impl WebStaticAdapter {
    pub fn new() -> Self {
        static CACHE: OnceLock<AdapterManifest> = OnceLock::new();
        let manifest = CACHE
            .get_or_init(|| {
                let yaml: serde_yaml::Value = serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
                    .expect("embedded adapter.yaml is valid YAML");
                let json: Value = serde_json::to_value(&yaml).expect("yaml->json");
                let errors = omni_core::schema::validate(
                    omni_core::schema::SchemaKind::Adapter,
                    &json,
                    "adapters/web-static/adapter.yaml",
                );
                assert!(
                    errors.is_empty(),
                    "embedded adapter manifest failed schema validation: {errors:?}"
                );
                serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
                    .expect("embedded adapter.yaml parses into AdapterManifest")
            })
            .clone();
        Self { manifest }
    }
}

impl Default for WebStaticAdapter {
    fn default() -> Self {
        Self::new()
    }
}

struct PreparedPage {
    html: String,
    css: Vec<String>,
    js: Vec<String>,
    assets: Vec<String>,
}

impl WebStaticAdapter {
    /// Renders one component (the root, or a composed child) into an HTML
    /// fragment: resolves its manifest, validates/merges `input_overrides`
    /// against its declared inputs, recursively renders every composition
    /// slot in `composes` (see `ComponentManifest::composes`) into trusted,
    /// pre-escaped HTML, then runs the component's own template through
    /// `template::render`. `stack` holds the chain of component names
    /// currently being rendered, purely as a defense-in-depth guard against
    /// runaway recursion — the resolver (`omni_core::resolver::resolve`)
    /// already guarantees `ctx.components` is acyclic, and
    /// `components.rs::discover_components` already requires every
    /// `composes` target to be a declared `dependencies` entry, so a real
    /// cycle here would mean that guarantee was somehow bypassed.
    fn render_component(
        &self,
        ctx: &BuildContext,
        component_name: &str,
        input_overrides: &BTreeMap<String, Value>,
        diags: &mut Diagnostics,
        stack: &mut Vec<String>,
    ) -> Option<String> {
        if stack.iter().any(|n| n == component_name) {
            diags.push(
                Diagnostic::error(
                    "E-WEB-COMPOSE-CYCLE",
                    format!(
                        "composition cycle detected: {} -> {component_name}",
                        stack.join(" -> ")
                    ),
                )
                .with_component(component_name.to_string())
                .with_target("web"),
            );
            return None;
        }

        let Some(resolved) = ctx.components.iter().find(|c| c.name == component_name) else {
            diags.push(
                Diagnostic::error(
                    "E-WEB-ROOT-MISSING",
                    format!("component '{component_name}' was not resolved for target 'web'"),
                )
                .with_component(component_name.to_string())
                .with_target("web"),
            );
            return None;
        };

        // Re-read the component manifest's declared interface + composes +
        // template source from disk (the resolver only kept the chosen
        // implementation + manifest_dir, not the full interface) so we can
        // validate inputs and slots.
        let component_yaml_path = resolved.manifest_dir.join("component.yaml");
        let component_raw = match fs::read_to_string(&component_yaml_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-WEB-COMPONENT-IO", format!("{e}"))
                        .with_file(component_yaml_path.display().to_string()),
                );
                return None;
            }
        };
        let component_manifest: omni_core::spec::component::ComponentManifest =
            match serde_yaml::from_str(&component_raw) {
                Ok(m) => m,
                Err(e) => {
                    diags.push(
                        Diagnostic::error("E-WEB-COMPONENT-PARSE", format!("{e}"))
                            .with_file(component_yaml_path.display().to_string()),
                    );
                    return None;
                }
            };

        let declared_inputs: BTreeSet<String> = component_manifest
            .interface
            .inputs
            .keys()
            .cloned()
            .collect();
        let declared: BTreeSet<String> = declared_inputs
            .iter()
            .cloned()
            .chain(component_manifest.composes.keys().cloned())
            .collect();
        let mut values: BTreeMap<String, template::SlotValue> = BTreeMap::new();

        for (name, field) in &component_manifest.interface.inputs {
            if let Some(default) = &field.default {
                values.insert(
                    name.clone(),
                    template::SlotValue::Escaped(json_scalar_to_string(default)),
                );
            }
        }
        for (name, value) in input_overrides {
            if !declared_inputs.contains(name) {
                diags.push(Diagnostic::warning(
                    "W-WEB-UNKNOWN-INPUT",
                    format!("input '{name}' is not declared on component '{component_name}'"),
                ));
                continue;
            }
            values.insert(
                name.clone(),
                template::SlotValue::Escaped(json_scalar_to_string(value)),
            );
        }
        for (name, field) in &component_manifest.interface.inputs {
            if field.required && !values.contains_key(name) {
                diags.push(
                    Diagnostic::error(
                        "E-WEB-MISSING-INPUT",
                        format!("required input '{name}' for component '{component_name}' was not provided and has no default"),
                    )
                    .with_component(component_name.to_string())
                    .with_target("web"),
                );
            }
        }
        if diags.has_errors() {
            return None;
        }

        stack.push(component_name.to_string());
        for (slot, compose) in &component_manifest.composes {
            let child_html =
                self.render_component(ctx, &compose.component, &compose.inputs, diags, stack);
            match child_html {
                Some(html) => {
                    values.insert(slot.clone(), template::SlotValue::Raw(html));
                }
                None => {
                    stack.pop();
                    return None;
                }
            }
        }
        stack.pop();

        let implementation = match component_manifest.implementations.get("web") {
            Some(i) => i,
            None => {
                diags.push(Diagnostic::error(
                    "E-WEB-NO-IMPLEMENTATION",
                    format!("component '{component_name}' has no web implementation"),
                ));
                return None;
            }
        };
        let template_path =
            match omni_core::paths::safe_join(&resolved.manifest_dir, &implementation.source) {
                Ok(p) => p,
                Err(e) => {
                    diags.push(Diagnostic::error("E-WEB-UNSAFE-SOURCE", format!("{e}")));
                    return None;
                }
            };
        let template_src = match fs::read_to_string(&template_path) {
            Ok(s) => s,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-WEB-TEMPLATE-IO",
                        format!("could not read template '{}': {e}", template_path.display()),
                    )
                    .with_file(template_path.display().to_string()),
                );
                return None;
            }
        };

        match template::render(&template_src, &values, &declared) {
            Ok(html) => Some(html),
            Err(errors) => {
                for e in errors {
                    diags.push(
                        Diagnostic::error("E-WEB-TEMPLATE-RENDER", e)
                            .with_file(template_path.display().to_string()),
                    );
                }
                None
            }
        }
    }

    fn prepare(&self, ctx: &BuildContext, diags: &mut Diagnostics) -> Option<PreparedPage> {
        let Some(entry_path) = &ctx.entry_path else {
            diags.push(
                Diagnostic::error(
                    "E-WEB-NO-ENTRY",
                    "target 'web' has no 'entry' configured in omni.yaml",
                )
                .with_target("web"),
            );
            return None;
        };
        if !entry_path.is_file() {
            diags.push(
                Diagnostic::error(
                    "E-WEB-ENTRY-MISSING",
                    format!("entry file '{}' does not exist", entry_path.display()),
                )
                .with_file(entry_path.display().to_string()),
            );
            return None;
        }

        let raw = match fs::read_to_string(entry_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-WEB-ENTRY-IO", format!("could not read entry file: {e}"))
                        .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-WEB-ENTRY-YAML", format!("invalid YAML: {e}"))
                        .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        let json_value: Value = serde_json::to_value(&yaml_value).unwrap_or(Value::Null);
        let schema_errors = omni_core::schema::validate(
            omni_core::schema::SchemaKind::WebApp,
            &json_value,
            &entry_path.display().to_string(),
        );
        if !schema_errors.is_empty() {
            for d in schema_errors {
                diags.push(d);
            }
            return None;
        }

        let app: WebAppManifest = match serde_yaml::from_value(yaml_value) {
            Ok(a) => a,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-WEB-ENTRY-PARSE",
                        format!("failed to parse web app manifest: {e}"),
                    )
                    .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        if app.schema != manifest::WEBAPP_SCHEMA_VERSION {
            diags.push(
                Diagnostic::error(
                    "E-WEB-ENTRY-SCHEMA-VERSION",
                    format!("unsupported web app manifest schema '{}'", app.schema),
                )
                .with_file(entry_path.display().to_string()),
            );
            return None;
        }

        let mut stack: Vec<String> = Vec::new();
        let fragment = self.render_component(
            ctx,
            &app.root.component,
            &app.root.inputs,
            diags,
            &mut stack,
        )?;

        // Verify every referenced public asset exists under <project>/public
        // before claiming success.
        let public_dir = ctx.project_root.join("public");
        let mut verify = |rel: &str, bucket: &str| -> Option<String> {
            match omni_core::paths::safe_join(&public_dir, rel) {
                Ok(p) if p.is_file() => Some(rel.to_string()),
                Ok(p) => {
                    diags.push(Diagnostic::error(
                        "E-WEB-ASSET-MISSING",
                        format!("{bucket} asset '{rel}' does not exist at '{}'", p.display()),
                    ));
                    None
                }
                Err(e) => {
                    diags.push(Diagnostic::error("E-WEB-UNSAFE-ASSET", format!("{e}")));
                    None
                }
            }
        };

        let css: Vec<String> = app
            .public
            .css
            .iter()
            .filter_map(|p| verify(p, "css"))
            .collect();
        let js: Vec<String> = app
            .public
            .js
            .iter()
            .filter_map(|p| verify(p, "js"))
            .collect();
        let assets: Vec<String> = app
            .public
            .assets
            .iter()
            .filter_map(|p| verify(p, "static"))
            .collect();

        if diags.has_errors() {
            return None;
        }

        let title = template::escape_html(&app.title);
        let css_links: String = css
            .iter()
            .map(|c| format!("\n    <link rel=\"stylesheet\" href=\"assets/{c}\">"))
            .collect();
        let js_tags: String = js
            .iter()
            .map(|j| format!("\n    <script src=\"assets/{j}\" defer></script>"))
            .collect();

        let html = format!(
            "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\">\n    <title>{title}</title>{css_links}\n  </head>\n  <body>\n{fragment}\n  </body>{js_tags}\n</html>\n"
        );

        Some(PreparedPage {
            html,
            css,
            js,
            assets,
        })
    }
}

fn json_scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

impl Adapter for WebStaticAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn validate(&self, ctx: &BuildContext) -> Diagnostics {
        let mut diags = Diagnostics::new();
        self.prepare(ctx, &mut diags);
        diags
    }

    fn compile(&self, ctx: &BuildContext) -> CompileOutput {
        let mut diags = Diagnostics::new();
        let Some(page) = self.prepare(ctx, &mut diags) else {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        };

        let mut artifacts = Vec::new();
        let write = |rel: &str, bytes: &[u8], diags: &mut Diagnostics| -> Option<Artifact> {
            let path = match ctx.output.join(rel) {
                Ok(p) => p,
                Err(e) => {
                    diags.push(Diagnostic::error("E-WEB-UNSAFE-OUTPUT", format!("{e}")));
                    return None;
                }
            };
            if let Some(parent) = path.parent() {
                if fs::create_dir_all(parent).is_err() {
                    diags.push(Diagnostic::error(
                        "E-WEB-WRITE-FAILED",
                        format!("could not create directory for '{rel}'"),
                    ));
                    return None;
                }
            }
            if fs::write(&path, bytes).is_err() {
                diags.push(Diagnostic::error(
                    "E-WEB-WRITE-FAILED",
                    format!("could not write '{rel}'"),
                ));
                return None;
            }
            Some(Artifact {
                relative_path: rel.to_string(),
                absolute_path: path,
            })
        };

        if let Some(a) = write("index.html", page.html.as_bytes(), &mut diags) {
            artifacts.push(a);
        }

        let public_dir = ctx.project_root.join("public");
        for asset in page
            .css
            .iter()
            .chain(page.js.iter())
            .chain(page.assets.iter())
        {
            let src = match omni_core::paths::safe_join(&public_dir, asset) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let bytes = match fs::read(&src) {
                Ok(b) => b,
                Err(e) => {
                    diags.push(Diagnostic::error(
                        "E-WEB-ASSET-IO",
                        format!("could not read '{}': {e}", src.display()),
                    ));
                    continue;
                }
            };
            if let Some(a) = write(&format!("assets/{asset}"), &bytes, &mut diags) {
                artifacts.push(a);
            }
        }

        if diags.has_errors() {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let build_manifest = serde_json::json!({
            "schema": "omniforge.dev/web-build-manifest/v1",
            "adapter": { "id": self.manifest.id, "version": self.manifest.version },
            "artifacts": artifacts.iter().map(|a| &a.relative_path).collect::<Vec<_>>(),
        });
        if let Some(a) = write(
            "build-manifest.json",
            serde_json::to_vec_pretty(&build_manifest)
                .unwrap_or_default()
                .as_slice(),
            &mut diags,
        ) {
            artifacts.push(a);
        }

        // Final verification: every claimed artifact must actually exist and
        // be readable before we call this a success.
        for artifact in &artifacts {
            match fs::metadata(&artifact.absolute_path) {
                Ok(m) if m.is_file() => {}
                _ => {
                    diags.push(Diagnostic::error(
                        "E-WEB-ARTIFACT-UNVERIFIED",
                        format!(
                            "artifact '{}' could not be verified on disk after writing",
                            artifact.relative_path
                        ),
                    ));
                }
            }
        }
        if diags.has_errors() {
            CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            }
        } else {
            CompileOutput {
                artifacts,
                diagnostics: diags,
            }
        }
    }
}
