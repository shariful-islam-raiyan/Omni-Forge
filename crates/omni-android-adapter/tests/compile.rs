//! End-to-end exercise of the android-gradle adapter against a synthetic
//! project tree, independent of the CLI.
//!
//! `builds_a_real_apk_end_to_end` genuinely invokes Gradle and asserts on a
//! real, produced APK — it is not mocked. If this machine's Android
//! toolchain (SDK + JDK + Gradle) isn't discoverable, the test prints why
//! and returns early rather than failing: a missing *local* toolchain is
//! an environment fact to report honestly (see
//! `docs/decisions/0011-android-gradle-adapter.md`), not a defect in this
//! adapter. On this project's own development machine, the toolchain *is*
//! present, so this test does perform a real build there.

use omni_adapter_api::{Adapter, BuildContext};
use omni_android_adapter::AndroidGradleAdapter;
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::{AdapterCapabilities, AdapterManifest};
use omni_core::spec::component::ComponentImplementation;
use std::fs;
use std::io::Read;
use tempfile::tempdir;

fn scaffold(root: &std::path::Path, message: &str) {
    fs::create_dir_all(root.join("lib/components/ui.app")).unwrap();
    fs::create_dir_all(root.join("android")).unwrap();

    fs::write(
        root.join("lib/components/ui.app/component.yaml"),
        r#"
schema: omniforge.dev/component/v1
name: ui.app
version: 1.0.0
kind: ui
interface:
  inputs:
    message:
      type: string
      required: true
implementations:
  android:
    adapter: android-gradle
    source: ./message.kt.tmpl
"#,
    )
    .unwrap();
    // Trailing newline deliberately included: a real authored template
    // file conventionally ends with one (most editors add it), and the
    // adapter must trim it rather than splice a raw newline into the
    // generated single-line Kotlin string literal.
    fs::write(
        root.join("lib/components/ui.app/message.kt.tmpl"),
        "{{message}}\n",
    )
    .unwrap();

    // Build the entry file as real YAML (via serde_yaml) rather than a
    // hand-escaped string, so a message containing quotes/backslashes
    // can't accidentally break out of the YAML scalar itself.
    let entry = serde_yaml::to_string(&serde_json::json!({
        "schema": "omniforge.dev/android-app/v1",
        "package": "dev.omniforge.testapp",
        "app_label": "Test App",
        "root": {
            "component": "ui.app",
            "inputs": { "message": message },
        },
    }))
    .unwrap();
    fs::write(root.join("android/app.yaml"), entry).unwrap();
}

fn make_ctx(root: &std::path::Path) -> BuildContext {
    let output = OutputDir::resolve(root, "dist").unwrap();
    let component = ResolvedComponent {
        name: "ui.app".into(),
        version: "1.0.0".into(),
        kind: omni_core::spec::component::ComponentKind::Ui,
        implementation: ComponentImplementation {
            adapter: "android-gradle".into(),
            source: "./message.kt.tmpl".into(),
        },
        manifest_dir: root.join("lib/components/ui.app"),
        adapter: AdapterManifest {
            schema: "omniforge.dev/adapter/v1".into(),
            id: "android-gradle".into(),
            version: "0.1.0".into(),
            protocol: "in-process/v1".into(),
            capabilities: AdapterCapabilities {
                languages: vec!["kotlin".into()],
                component_kinds: vec!["ui".into()],
                targets: vec!["android".into()],
                operations: vec!["validate".into(), "compile".into()],
                requires: vec![],
            },
            dev: None,
        },
    };
    BuildContext {
        project_root: root.to_path_buf(),
        target: "android".into(),
        entry_path: Some(root.join("android/app.yaml")),
        output,
        components: vec![component],
    }
}

#[test]
fn missing_required_input_fails_the_build_without_partial_output() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root, "irrelevant");
    fs::write(
        root.join("android/app.yaml"),
        r#"
schema: omniforge.dev/android-app/v1
package: dev.omniforge.testapp
app_label: "Test App"
root:
  component: ui.app
"#,
    )
    .unwrap();
    let ctx = make_ctx(&root);

    let adapter = AndroidGradleAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result.artifacts.is_empty());
    assert!(!root.join("dist/app-debug.apk").exists());
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = AndroidGradleAdapter::new();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn builds_a_real_apk_end_to_end() {
    // Gated behind an explicit opt-in env var, deliberately *in addition
    // to* toolchain detection: unlike `python3` (near-universally present
    // on CI/dev machines, confirmed by this project's own existing
    // tests), a *working* Android SDK/JDK/Gradle/AGP/Kotlin combination on
    // an arbitrary machine is a much bigger assumption this milestone
    // could not verify on GitHub-hosted CI runners. Running this test
    // unconditionally risks a real version-incompatibility failure (not a
    // bug in this adapter, but a CI-image mismatch) breaking the shared
    // `cargo test --workspace` gate for every contributor. `omni doctor`
    // and `scripts/verify.sh` still exercise the real thing on a machine
    // that opts in (see `docs/decisions/0011-android-gradle-adapter.md`
    // and `.github/workflows/ci.yml`'s separate, isolated `android` job).
    if std::env::var("OMNIFORGE_RUN_ANDROID_E2E").is_err() {
        eprintln!(
            "SKIPPING builds_a_real_apk_end_to_end: set OMNIFORGE_RUN_ANDROID_E2E=1 to run the \
             real Gradle build (see docs/decisions/0011-android-gradle-adapter.md)."
        );
        return;
    }
    if let Err(problems) = omni_android_adapter::toolchain::detect() {
        eprintln!(
            "SKIPPING builds_a_real_apk_end_to_end: Android toolchain not available on this \
             machine (missing: {}). This is an environment limitation, not a test failure — \
             see docs/decisions/0011-android-gradle-adapter.md.",
            problems.missing.join("; ")
        );
        return;
    }

    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    // A deliberately script-looking message: proves this ends up as a
    // harmless Kotlin string literal, not interpolated/executed code.
    scaffold(&root, "Hello ${System.exit(1)} \"quoted\"");
    let ctx = make_ctx(&root);

    let adapter = AndroidGradleAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );
    assert_eq!(result.artifacts.len(), 2);

    let apk_path = root.join("dist/app-debug.apk");
    assert!(apk_path.is_file());
    let metadata = fs::metadata(&apk_path).unwrap();
    assert!(
        metadata.len() > 50_000,
        "APK is suspiciously small ({} bytes) to be a real compiled app",
        metadata.len()
    );

    let file = fs::File::open(&apk_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    assert!(archive.by_name("AndroidManifest.xml").is_ok());
    let mut dex = archive.by_name("classes.dex").unwrap();
    let mut buf = Vec::new();
    dex.read_to_end(&mut buf).unwrap();
    assert!(!buf.is_empty(), "classes.dex inside the APK is empty");

    let manifest_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("dist/build-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest_json["adapter"]["id"], "android-gradle");
    assert_eq!(manifest_json["package"], "dev.omniforge.testapp");
}
