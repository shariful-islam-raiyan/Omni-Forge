//! The `windows-desktop` adapter: the first OmniForge target that isn't
//! `web`, `backend`, or `android`.
//!
//! Scope, stated as plainly as `web-static`'s (ADR 0003) and
//! `android-gradle`'s (ADR 0011): one project, one root UI component,
//! rendered through the exact same fixed `{{field}}`-substitution
//! template style (`rust_template.rs`, a direct sibling of
//! `omni-android-adapter`'s `kotlin.rs`) into a single, real Rust source
//! file, compiled by a real `rustc` into a real native Windows
//! executable, which is then genuinely **run** (not merely checked for
//! existence) and its captured stdout kept as a build artifact — the same
//! "prove it actually executes" discipline the backend adapters use. No
//! GUI window, no dialog, no windowing toolkit: this proves native
//! Windows executable compilation and execution for the `windows` target
//! for real; a real GUI surface is Planned, not faked. See
//! `docs/decisions/0016-windows-desktop-adapter.md`.
//!
//! Architecturally, this is the same adapter *style* as `android-gradle`:
//! in-process Rust (`protocol: in-process/v1`) that safely shells out to
//! a real external tool (`rustc`, via `omni_adapter_api::process::
//! run_tool` — the exact same no-shell/controlled-env/bounded-timeout
//! primitive `android-gradle` uses for Gradle) rather than speaking the
//! out-of-process wire protocol.

mod manifest;
mod rust_template;

pub use manifest::{RootRef, WindowsAppManifest};

use omni_adapter_api::process::{controlled_env, resolve_executable, run_tool};
use omni_adapter_api::{Adapter, Artifact, BuildContext, CompileOutput};
use omni_core::diagnostics::{Diagnostic, Diagnostics};
use omni_core::spec::adapter::AdapterManifest;
use omni_core::spec::component::ComponentManifest;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;

const ADAPTER_MANIFEST_SRC: &str = include_str!("../../../adapters/windows-desktop/adapter.yaml");
const COMPILE_TIMEOUT: Duration = Duration::from_secs(60);
const RUN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct WindowsDesktopAdapter {
    manifest: AdapterManifest,
}

impl WindowsDesktopAdapter {
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
                    "adapters/windows-desktop/adapter.yaml",
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

impl Default for WindowsDesktopAdapter {
    fn default() -> Self {
        Self::new()
    }
}

struct PreparedSource {
    rust_source: String,
    app_name: String,
}

fn json_scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

impl WindowsDesktopAdapter {
    fn prepare(&self, ctx: &BuildContext, diags: &mut Diagnostics) -> Option<PreparedSource> {
        let Some(entry_path) = &ctx.entry_path else {
            diags.push(
                Diagnostic::error(
                    "E-WINDOWS-NO-ENTRY",
                    "target 'windows' has no 'entry' configured in omni.yaml",
                )
                .with_target("windows"),
            );
            return None;
        };
        if !entry_path.is_file() {
            diags.push(
                Diagnostic::error(
                    "E-WINDOWS-ENTRY-MISSING",
                    format!("entry file '{}' does not exist", entry_path.display()),
                )
                .with_file(entry_path.display().to_string()),
            );
            return None;
        }

        let raw = match fs::read_to_string(entry_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(Diagnostic::error(
                    "E-WINDOWS-ENTRY-IO",
                    format!("could not read entry file: {e}"),
                ));
                return None;
            }
        };
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                diags.push(Diagnostic::error(
                    "E-WINDOWS-ENTRY-YAML",
                    format!("invalid YAML: {e}"),
                ));
                return None;
            }
        };
        let json_value: Value = serde_json::to_value(&yaml_value).unwrap_or(Value::Null);
        let schema_errors = omni_core::schema::validate(
            omni_core::schema::SchemaKind::WindowsApp,
            &json_value,
            &entry_path.display().to_string(),
        );
        if !schema_errors.is_empty() {
            for d in schema_errors {
                diags.push(d);
            }
            return None;
        }

        let app: WindowsAppManifest = match serde_yaml::from_value(yaml_value) {
            Ok(a) => a,
            Err(e) => {
                diags.push(Diagnostic::error(
                    "E-WINDOWS-ENTRY-PARSE",
                    format!("failed to parse windows app manifest: {e}"),
                ));
                return None;
            }
        };
        if app.schema != manifest::WINDOWS_APP_SCHEMA_VERSION {
            diags.push(Diagnostic::error(
                "E-WINDOWS-ENTRY-SCHEMA-VERSION",
                format!("unsupported windows app manifest schema '{}'", app.schema),
            ));
            return None;
        }

        let component_name = &app.root.component;
        let Some(resolved) = ctx.components.iter().find(|c| &c.name == component_name) else {
            diags.push(
                Diagnostic::error(
                    "E-WINDOWS-ROOT-MISSING",
                    format!("component '{component_name}' was not resolved for target 'windows'"),
                )
                .with_component(component_name.clone())
                .with_target("windows"),
            );
            return None;
        };

        let component_yaml_path = resolved.manifest_dir.join("component.yaml");
        let component_raw = match fs::read_to_string(&component_yaml_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-WINDOWS-COMPONENT-IO", format!("{e}"))
                        .with_file(component_yaml_path.display().to_string()),
                );
                return None;
            }
        };
        let component_manifest: ComponentManifest = match serde_yaml::from_str(&component_raw) {
            Ok(m) => m,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-WINDOWS-COMPONENT-PARSE", format!("{e}"))
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
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for (name, field) in &component_manifest.interface.inputs {
            if let Some(default) = &field.default {
                values.insert(name.clone(), json_scalar_to_string(default));
            }
        }
        for (name, value) in &app.root.inputs {
            if !declared_inputs.contains(name) {
                diags.push(Diagnostic::warning(
                    "W-WINDOWS-UNKNOWN-INPUT",
                    format!("input '{name}' is not declared on component '{component_name}'"),
                ));
                continue;
            }
            values.insert(name.clone(), json_scalar_to_string(value));
        }
        for (name, field) in &component_manifest.interface.inputs {
            if field.required && !values.contains_key(name) {
                diags.push(
                    Diagnostic::error(
                        "E-WINDOWS-MISSING-INPUT",
                        format!("required input '{name}' for component '{component_name}' was not provided and has no default"),
                    )
                    .with_component(component_name.clone())
                    .with_target("windows"),
                );
            }
        }
        if diags.has_errors() {
            return None;
        }

        let implementation = match component_manifest.implementations.get("windows") {
            Some(i) => i,
            None => {
                diags.push(Diagnostic::error(
                    "E-WINDOWS-NO-IMPLEMENTATION",
                    format!("component '{component_name}' has no windows implementation"),
                ));
                return None;
            }
        };
        let template_path =
            match omni_core::paths::safe_join(&resolved.manifest_dir, &implementation.source) {
                Ok(p) => p,
                Err(e) => {
                    diags.push(Diagnostic::error("E-WINDOWS-UNSAFE-SOURCE", format!("{e}")));
                    return None;
                }
            };
        let template_src = match fs::read_to_string(&template_path) {
            Ok(s) => s,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-WINDOWS-TEMPLATE-IO",
                        format!("could not read template '{}': {e}", template_path.display()),
                    )
                    .with_file(template_path.display().to_string()),
                );
                return None;
            }
        };

        match rust_template::render(&template_src, &values, &declared_inputs) {
            Ok(rust_source) => Some(PreparedSource {
                rust_source,
                app_name: app.app_name,
            }),
            Err(errors) => {
                for e in errors {
                    diags.push(
                        Diagnostic::error("E-WINDOWS-TEMPLATE-RENDER", e)
                            .with_file(template_path.display().to_string()),
                    );
                }
                None
            }
        }
    }

    fn exe_name(base: &str) -> String {
        if cfg!(windows) {
            format!("{base}.exe")
        } else {
            base.to_string()
        }
    }
}

impl Adapter for WindowsDesktopAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn validate(&self, ctx: &BuildContext) -> Diagnostics {
        let mut diags = Diagnostics::new();
        let Some(prepared) = self.prepare(ctx, &mut diags) else {
            return diags;
        };

        // A real compile-check, not just template rendering: `rustc
        // --emit=metadata` type-checks without producing a runnable
        // binary, writing nothing lasting (the temp dir is discarded
        // immediately after) -- the same "validate() must not write any
        // files" discipline `typescript-backend`'s `tsc --noEmit` and
        // `go-backend`'s discarded temp build both honor. Without this,
        // `omni check` would report success for Rust source `rustc`
        // itself would reject -- a real gap, found and closed here, not
        // assumed harmless.
        let Some(rustc) = resolve_executable(&["rustc".to_string()]) else {
            diags.push(
                Diagnostic::error(
                    "E-ADAPTER-TOOLCHAIN-MISSING",
                    "no usable 'rustc' found on PATH",
                )
                .with_target("windows")
                .with_suggestion(
                    "install Rust (rustup.rs) and ensure 'rustc' is on PATH, or run 'omni doctor'",
                ),
            );
            return diags;
        };
        let Ok(temp_dir) = tempfile::tempdir() else {
            diags.push(Diagnostic::error(
                "E-WINDOWS-WRITE-FAILED",
                "could not create a temporary directory for type-checking",
            ));
            return diags;
        };
        let source_path = temp_dir.path().join("source.rs");
        if fs::write(&source_path, &prepared.rust_source).is_err() {
            diags.push(Diagnostic::error(
                "E-WINDOWS-WRITE-FAILED",
                "could not write a temporary source file for type-checking",
            ));
            return diags;
        }
        let metadata_path = temp_dir.path().join("check.rmeta");
        let check_args = vec![
            source_path.display().to_string(),
            "--emit=metadata".to_string(),
            "-o".to_string(),
            metadata_path.display().to_string(),
        ];
        match run_tool(
            &rustc,
            &check_args,
            temp_dir.path(),
            &controlled_env(),
            COMPILE_TIMEOUT,
        ) {
            Ok(output) if !output.success => {
                diags.push(
                    Diagnostic::error(
                        "E-WINDOWS-COMPILE-FAILED",
                        "rustc reported compile errors for the rendered component source",
                    )
                    .with_explanation(output.stderr)
                    .with_target("windows"),
                );
            }
            Ok(_) => {}
            Err(d) => diags.push(d.with_target("windows")),
        }
        diags
    }

    fn compile(&self, ctx: &BuildContext) -> CompileOutput {
        let mut diags = Diagnostics::new();
        let Some(prepared) = self.prepare(ctx, &mut diags) else {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        };

        let Some(rustc) = resolve_executable(&["rustc".to_string()]) else {
            diags.push(
                Diagnostic::error(
                    "E-ADAPTER-TOOLCHAIN-MISSING",
                    "no usable 'rustc' found on PATH",
                )
                .with_target("windows")
                .with_suggestion(
                    "install Rust (rustup.rs) and ensure 'rustc' is on PATH, or run 'omni doctor'",
                ),
            );
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        };

        let write = |rel: &str, bytes: &[u8], diags: &mut Diagnostics| -> Option<Artifact> {
            let path = match ctx.output.join(rel) {
                Ok(p) => p,
                Err(e) => {
                    diags.push(Diagnostic::error("E-WINDOWS-UNSAFE-OUTPUT", format!("{e}")));
                    return None;
                }
            };
            if let Some(parent) = path.parent() {
                if fs::create_dir_all(parent).is_err() {
                    diags.push(Diagnostic::error(
                        "E-WINDOWS-WRITE-FAILED",
                        format!("could not create directory for '{rel}'"),
                    ));
                    return None;
                }
            }
            if fs::write(&path, bytes).is_err() {
                diags.push(Diagnostic::error(
                    "E-WINDOWS-WRITE-FAILED",
                    format!("could not write '{rel}'"),
                ));
                return None;
            }
            Some(Artifact {
                relative_path: rel.to_string(),
                absolute_path: path,
            })
        };

        let mut artifacts = Vec::new();
        let Some(source_artifact) = write("source.rs", prepared.rust_source.as_bytes(), &mut diags)
        else {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        };
        artifacts.push(source_artifact.clone());

        let app_dir = match ctx.output.join("app") {
            Ok(p) => p,
            Err(e) => {
                diags.push(Diagnostic::error("E-WINDOWS-UNSAFE-OUTPUT", format!("{e}")));
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };
        if fs::create_dir_all(&app_dir).is_err() {
            diags.push(Diagnostic::error(
                "E-WINDOWS-WRITE-FAILED",
                "could not create 'app/' output directory",
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }
        let exe_path = app_dir.join(Self::exe_name(&prepared.app_name));

        let build_args = vec![
            source_artifact.absolute_path.display().to_string(),
            "-O".to_string(),
            "-o".to_string(),
            exe_path.display().to_string(),
        ];
        let build_result = run_tool(
            &rustc,
            &build_args,
            ctx.output.path(),
            &controlled_env(),
            COMPILE_TIMEOUT,
        );
        let build_output = match build_result {
            Ok(o) => o,
            Err(d) => {
                diags.push(d.with_target("windows"));
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };
        if !build_output.success || !exe_path.is_file() {
            diags.push(
                Diagnostic::error(
                    "E-WINDOWS-COMPILE-FAILED",
                    "rustc reported compile errors for the rendered component source",
                )
                .with_explanation(build_output.stderr)
                .with_target("windows"),
            );
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }
        artifacts.push(Artifact {
            relative_path: format!("app/{}", Self::exe_name(&prepared.app_name)),
            absolute_path: exe_path.clone(),
        });

        // Genuinely *run* the compiled native executable -- proof it
        // actually launches and executes, not merely that a file with an
        // .exe extension exists. Captured stdout is kept as a real,
        // inspectable artifact, the same discipline the backend adapters
        // use.
        let run_result = run_tool(
            &exe_path.display().to_string(),
            &[],
            ctx.output.path(),
            &controlled_env(),
            RUN_TIMEOUT,
        );
        let run_output = match run_result {
            Ok(o) => o,
            Err(d) => {
                diags.push(d.with_target("windows"));
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };
        if !run_output.success {
            diags.push(
                Diagnostic::error(
                    "E-WINDOWS-EXECUTION-FAILED",
                    "the compiled executable exited with a non-zero status",
                )
                .with_explanation(run_output.stderr)
                .with_target("windows"),
            );
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }
        if let Some(a) = write("output.txt", run_output.stdout.as_bytes(), &mut diags) {
            artifacts.push(a);
        }

        if diags.has_errors() {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let build_manifest = serde_json::json!({
            "schema": "omniforge.dev/windows-build-manifest/v1",
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

        for artifact in &artifacts {
            match fs::metadata(&artifact.absolute_path) {
                Ok(m) if m.is_file() => {}
                _ => {
                    diags.push(Diagnostic::error(
                        "E-WINDOWS-ARTIFACT-UNVERIFIED",
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
