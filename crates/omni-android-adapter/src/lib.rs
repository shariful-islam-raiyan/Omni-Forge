//! The `android-gradle` adapter.
//!
//! Scope (see `docs/decisions/0011-android-gradle-adapter.md` for the full
//! rationale): renders one root UI component into a minimal, real Android
//! Gradle project (one `Activity`, one `TextView`, a component-supplied
//! message substituted the same fixed `{{field}}` way `web-static` does —
//! see `kotlin.rs`), then actually invokes a real, locally-discovered
//! Gradle to produce a real, verified debug APK. There is no emulator/
//! device install, no release signing, and no AAB output — see the ADR's
//! Consequences section for exactly what's left.
//!
//! Unlike `web-static` (fully in-process, no subprocess) and
//! `python-backend` (an out-of-process adapter speaking OmniForge's own
//! wire protocol), this adapter is in-process Rust that safely shells out
//! to a real external build tool (Gradle) that does not speak that
//! protocol — using `omni_adapter_api::process::run_tool`, the same
//! no-shell/controlled-environment/bounded-timeout discipline every other
//! subprocess in this project uses, just without the JSON request/response
//! framing `execute()` expects.

mod kotlin;
mod manifest;
mod project;
pub mod toolchain;

pub use manifest::{AndroidAppManifest, RootRef};

use omni_adapter_api::{process, Adapter, Artifact, BuildContext, CompileOutput};
use omni_core::diagnostics::{Diagnostic, Diagnostics};
use omni_core::spec::adapter::AdapterManifest;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

const ADAPTER_MANIFEST_SRC: &str = include_str!("../../../adapters/android-gradle/adapter.yaml");

/// Cold Gradle invocations (plugin/dependency resolution on a machine that
/// hasn't built this exact project before) can genuinely take a few
/// minutes; bounded generously rather than risking a false timeout on a
/// real, still-progressing build.
const GRADLE_TIMEOUT: Duration = Duration::from_secs(600);

pub struct AndroidGradleAdapter {
    manifest: AdapterManifest,
}

impl AndroidGradleAdapter {
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
                    "adapters/android-gradle/adapter.yaml",
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

impl Default for AndroidGradleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

struct PreparedApp {
    package: String,
    app_label: String,
    min_sdk: u32,
    target_sdk: u32,
    compile_sdk: u32,
    escaped_message: String,
}

impl AndroidGradleAdapter {
    fn prepare(&self, ctx: &BuildContext, diags: &mut Diagnostics) -> Option<PreparedApp> {
        let Some(entry_path) = &ctx.entry_path else {
            diags.push(
                Diagnostic::error(
                    "E-ANDROID-NO-ENTRY",
                    "target 'android' has no 'entry' configured in omni.yaml",
                )
                .with_target("android"),
            );
            return None;
        };
        if !entry_path.is_file() {
            diags.push(
                Diagnostic::error(
                    "E-ANDROID-ENTRY-MISSING",
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
                    Diagnostic::error(
                        "E-ANDROID-ENTRY-IO",
                        format!("could not read entry file: {e}"),
                    )
                    .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-ANDROID-ENTRY-YAML", format!("invalid YAML: {e}"))
                        .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        let json_value: Value = serde_json::to_value(&yaml_value).unwrap_or(Value::Null);
        let schema_errors = omni_core::schema::validate(
            omni_core::schema::SchemaKind::AndroidApp,
            &json_value,
            &entry_path.display().to_string(),
        );
        if !schema_errors.is_empty() {
            for d in schema_errors {
                diags.push(d);
            }
            return None;
        }

        let app: AndroidAppManifest = match serde_yaml::from_value(yaml_value) {
            Ok(a) => a,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-ANDROID-ENTRY-PARSE",
                        format!("failed to parse android app manifest: {e}"),
                    )
                    .with_file(entry_path.display().to_string()),
                );
                return None;
            }
        };
        if app.schema != manifest::ANDROID_APP_SCHEMA_VERSION {
            diags.push(
                Diagnostic::error(
                    "E-ANDROID-ENTRY-SCHEMA-VERSION",
                    format!("unsupported android app manifest schema '{}'", app.schema),
                )
                .with_file(entry_path.display().to_string()),
            );
            return None;
        }

        let Some(root_component) = ctx.components.iter().find(|c| c.name == app.root.component)
        else {
            diags.push(
                Diagnostic::error(
                    "E-ANDROID-ROOT-MISSING",
                    format!(
                        "root component '{}' was not resolved for target 'android'",
                        app.root.component
                    ),
                )
                .with_component(app.root.component.clone())
                .with_target("android"),
            );
            return None;
        };

        let component_yaml_path = root_component.manifest_dir.join("component.yaml");
        let component_raw = match fs::read_to_string(&component_yaml_path) {
            Ok(r) => r,
            Err(e) => {
                diags.push(
                    Diagnostic::error("E-ANDROID-COMPONENT-IO", format!("{e}"))
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
                        Diagnostic::error("E-ANDROID-COMPONENT-PARSE", format!("{e}"))
                            .with_file(component_yaml_path.display().to_string()),
                    );
                    return None;
                }
            };

        let declared: BTreeSet<String> = component_manifest
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
            if !declared.contains(name) {
                diags.push(Diagnostic::warning(
                    "W-ANDROID-UNKNOWN-INPUT",
                    format!(
                        "input '{name}' is not declared on component '{}'",
                        app.root.component
                    ),
                ));
                continue;
            }
            values.insert(name.clone(), json_scalar_to_string(value));
        }
        for (name, field) in &component_manifest.interface.inputs {
            if field.required && !values.contains_key(name) {
                diags.push(
                    Diagnostic::error(
                        "E-ANDROID-MISSING-INPUT",
                        format!("required input '{name}' for component '{}' was not provided and has no default", app.root.component),
                    )
                    .with_component(app.root.component.clone())
                    .with_target("android"),
                );
            }
        }
        if diags.has_errors() {
            return None;
        }

        let implementation = match component_manifest.implementations.get("android") {
            Some(i) => i,
            None => {
                diags.push(Diagnostic::error(
                    "E-ANDROID-NO-IMPLEMENTATION",
                    format!(
                        "component '{}' has no android implementation",
                        app.root.component
                    ),
                ));
                return None;
            }
        };
        let template_path =
            match omni_core::paths::safe_join(&root_component.manifest_dir, &implementation.source)
            {
                Ok(p) => p,
                Err(e) => {
                    diags.push(Diagnostic::error("E-ANDROID-UNSAFE-SOURCE", format!("{e}")));
                    return None;
                }
            };
        let template_src = match fs::read_to_string(&template_path) {
            Ok(s) => s,
            Err(e) => {
                diags.push(
                    Diagnostic::error(
                        "E-ANDROID-TEMPLATE-IO",
                        format!("could not read template '{}': {e}", template_path.display()),
                    )
                    .with_file(template_path.display().to_string()),
                );
                return None;
            }
        };

        // A template file conventionally ends with a trailing newline (most
        // editors/`Write` tools add one); the *displayed message* a
        // component author means to author is virtually never meant to
        // include that final newline as literal content, and since the
        // entire rendered result is spliced into one single-line Kotlin
        // string literal (see `project.rs::main_activity_kt`), a raw
        // newline there would make the generated Kotlin source invalid.
        let template_src = template_src.trim_end_matches(['\n', '\r']);
        let escaped_message = match kotlin::render(template_src, &values, &declared) {
            Ok(rendered) => rendered,
            Err(errors) => {
                for e in errors {
                    diags.push(
                        Diagnostic::error("E-ANDROID-TEMPLATE-RENDER", e)
                            .with_file(template_path.display().to_string()),
                    );
                }
                return None;
            }
        };

        let min_sdk = app.min_sdk();
        let target_sdk = app.target_sdk();
        let compile_sdk = app.compile_sdk();
        Some(PreparedApp {
            package: app.package,
            app_label: app.app_label,
            min_sdk,
            target_sdk,
            compile_sdk,
            escaped_message,
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

/// Opens `apk_path` as a real zip archive and confirms it contains a
/// nonempty `AndroidManifest.xml` entry — the minimum real check that this
/// is an actual Android package, not merely a file that exists with the
/// right name.
fn verify_apk(apk_path: &std::path::Path) -> Result<(), String> {
    let file = fs::File::open(apk_path).map_err(|e| format!("could not open APK: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("not a valid zip archive: {e}"))?;
    let mut manifest_entry = archive
        .by_name("AndroidManifest.xml")
        .map_err(|_| "APK does not contain AndroidManifest.xml".to_string())?;
    let mut buf = Vec::new();
    manifest_entry
        .read_to_end(&mut buf)
        .map_err(|e| format!("could not read AndroidManifest.xml from APK: {e}"))?;
    if buf.is_empty() {
        return Err("AndroidManifest.xml inside the APK is empty".to_string());
    }
    Ok(())
}

impl Adapter for AndroidGradleAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    /// Fast, offline validation only (parses/validates the entry file,
    /// resolves the root component, checks inputs, renders the template) —
    /// it does not invoke Gradle. The real Android toolchain is only
    /// required by `compile()`; `omni check`/`omni doctor` is how a missing
    /// SDK/JDK/Gradle is surfaced, not `validate()`.
    fn validate(&self, ctx: &BuildContext) -> Diagnostics {
        let mut diags = Diagnostics::new();
        self.prepare(ctx, &mut diags);
        diags
    }

    fn compile(&self, ctx: &BuildContext) -> CompileOutput {
        let mut diags = Diagnostics::new();
        let Some(app) = self.prepare(ctx, &mut diags) else {
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        };

        let toolchain = match toolchain::detect() {
            Ok(t) => t,
            Err(problems) => {
                diags.push(
                    Diagnostic::error(
                        "E-ANDROID-TOOLCHAIN-MISSING",
                        format!(
                            "the Android toolchain is incomplete; missing: {}",
                            problems.missing.join("; ")
                        ),
                    )
                    .with_target("android")
                    .with_suggestion("run 'omni doctor' for details"),
                );
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };

        // A scratch Gradle project, deliberately outside `ctx.output`: the
        // target's real build output should be the APK (and a manifest),
        // not Gradle's own multi-megabyte intermediate `build/` tree. Not
        // wiped between builds -- Gradle's own incremental build/cache
        // state inside it is a real, valid speedup for repeated builds of
        // the same project, exactly as it would be for a developer running
        // Gradle by hand.
        let project_dir = ctx
            .project_root
            .join(".omniforge")
            .join("android-build")
            .join(&ctx.target);
        if let Err(e) = fs::create_dir_all(&project_dir) {
            diags.push(Diagnostic::error(
                "E-ANDROID-SCRATCH-DIR",
                format!("could not create Gradle project directory: {e}"),
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let spec = project::ProjectSpec {
            package: &app.package,
            app_label: &app.app_label,
            min_sdk: app.min_sdk,
            target_sdk: app.target_sdk,
            compile_sdk: app.compile_sdk,
            sdk_dir: &toolchain.sdk_dir,
            escaped_message: &app.escaped_message,
        };
        if let Err(e) = project::generate(&spec, &project_dir) {
            diags.push(Diagnostic::error(
                "E-ANDROID-PROJECT-GENERATE",
                format!("could not generate Gradle project: {e}"),
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let gradle_exe = toolchain.gradle_exe.display().to_string();
        // Deliberately no explicit `--project-dir`: `ctx.project_root` (and
        // therefore `project_dir`, derived from it) is a canonicalized,
        // `\\?\`-prefixed extended-length path on Windows
        // (`omni_core::loader::find_project_root`); Gradle's own Java-side
        // `File.getCanonicalPath()` cannot parse that prefix and fails with
        // a raw `java.io.IOException: Invalid file path` if it's ever
        // passed as a *string argument*. Setting the child process's
        // working directory to `project_dir` (via `run_tool`'s `cwd`, a
        // real `CreateProcess` call that handles `\\?\` paths natively) and
        // letting Gradle discover its project from the cwd sidesteps this
        // entirely.
        let args = vec!["assembleDebug".to_string(), "--no-daemon".to_string()];
        let env = process::controlled_env_with(&[
            (
                "JAVA_HOME".to_string(),
                toolchain.java_home.display().to_string(),
            ),
            (
                "ANDROID_HOME".to_string(),
                toolchain.sdk_dir.display().to_string(),
            ),
            (
                "ANDROID_SDK_ROOT".to_string(),
                toolchain.sdk_dir.display().to_string(),
            ),
        ]);

        let output = match process::run_tool(&gradle_exe, &args, &project_dir, &env, GRADLE_TIMEOUT)
        {
            Ok(o) => o,
            Err(d) => {
                diags.push(d);
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };

        if !output.success {
            diags.push(
                Diagnostic::error("E-ANDROID-BUILD-FAILED", "Gradle build failed")
                    .with_explanation(truncate_tail(
                        &format!("{}\n{}", output.stdout, output.stderr),
                        4000,
                    )),
            );
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let apk_path = project_dir.join("app/build/outputs/apk/debug/app-debug.apk");
        if !apk_path.is_file() {
            diags.push(Diagnostic::error(
                "E-ANDROID-APK-MISSING",
                format!(
                    "Gradle reported success but no APK was found at '{}'",
                    apk_path.display()
                ),
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }
        if let Err(e) = verify_apk(&apk_path) {
            diags.push(Diagnostic::error(
                "E-ANDROID-APK-INVALID",
                format!("produced APK failed verification: {e}"),
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }

        let mut artifacts = Vec::new();
        let dest = match ctx.output.join("app-debug.apk") {
            Ok(p) => p,
            Err(e) => {
                diags.push(Diagnostic::error("E-ANDROID-UNSAFE-OUTPUT", format!("{e}")));
                return CompileOutput {
                    artifacts: Vec::new(),
                    diagnostics: diags,
                };
            }
        };
        if let Some(parent) = dest.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::copy(&apk_path, &dest) {
            diags.push(Diagnostic::error(
                "E-ANDROID-WRITE-FAILED",
                format!("could not copy APK to build output: {e}"),
            ));
            return CompileOutput {
                artifacts: Vec::new(),
                diagnostics: diags,
            };
        }
        artifacts.push(Artifact {
            relative_path: "app-debug.apk".to_string(),
            absolute_path: dest,
        });

        let build_manifest = serde_json::json!({
            "schema": "omniforge.dev/android-build-manifest/v1",
            "adapter": { "id": self.manifest.id, "version": self.manifest.version },
            "package": app.package,
            "min_sdk": app.min_sdk,
            "target_sdk": app.target_sdk,
            "compile_sdk": app.compile_sdk,
            "artifacts": ["app-debug.apk"],
        });
        if let Ok(manifest_path) = ctx.output.join("build-manifest.json") {
            if fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&build_manifest).unwrap_or_default(),
            )
            .is_ok()
            {
                artifacts.push(Artifact {
                    relative_path: "build-manifest.json".to_string(),
                    absolute_path: manifest_path,
                });
            }
        }

        // Final verification: every claimed artifact must actually exist
        // and be readable before this is called a success.
        for artifact in &artifacts {
            match fs::metadata(&artifact.absolute_path) {
                Ok(m) if m.is_file() => {}
                _ => {
                    diags.push(Diagnostic::error(
                        "E-ANDROID-ARTIFACT-UNVERIFIED",
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

fn truncate_tail(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        format!("...{}", &trimmed[trimmed.len() - max..])
    }
}
