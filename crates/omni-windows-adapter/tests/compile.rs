//! End-to-end exercise of the `windows-desktop` adapter against a
//! synthetic project tree, independent of the CLI -- mirrors
//! `omni-web-adapter/tests/compile.rs`'s structure. Proves the adapter
//! genuinely compiles rendered Rust source with a real `rustc` into a
//! real native executable, then genuinely runs it.

use omni_adapter_api::{Adapter, BuildContext};
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::{AdapterCapabilities, AdapterManifest};
use omni_core::spec::component::ComponentImplementation;
use omni_windows_adapter::WindowsDesktopAdapter;
use std::fs;
use tempfile::tempdir;

fn scaffold(root: &std::path::Path, template: &str) {
    fs::create_dir_all(root.join("lib/components/ui.greeting")).unwrap();
    fs::create_dir_all(root.join("windows")).unwrap();

    fs::write(
        root.join("lib/components/ui.greeting/component.yaml"),
        r#"
schema: omniforge.dev/component/v1
name: ui.greeting
version: 1.0.0
kind: ui
interface:
  inputs:
    message:
      type: string
      required: true
implementations:
  windows:
    adapter: windows-desktop
    source: ./main.rs.tmpl
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.greeting/main.rs.tmpl"),
        template,
    )
    .unwrap();

    fs::write(
        root.join("windows/app.yaml"),
        r#"
schema: omniforge.dev/windows-app/v1
app_name: test-app
root:
  component: ui.greeting
  inputs:
    message: "Hi <there> & welcome"
"#,
    )
    .unwrap();
}

fn make_ctx(root: &std::path::Path) -> BuildContext {
    let output = OutputDir::resolve(root, "dist").unwrap();
    let component = ResolvedComponent {
        name: "ui.greeting".into(),
        version: "1.0.0".into(),
        kind: omni_core::spec::component::ComponentKind::Ui,
        implementation: ComponentImplementation {
            adapter: "windows-desktop".into(),
            source: "./main.rs.tmpl".into(),
        },
        manifest_dir: root.join("lib/components/ui.greeting"),
        adapter: AdapterManifest {
            schema: "omniforge.dev/adapter/v1".into(),
            id: "windows-desktop".into(),
            version: "0.1.0".into(),
            protocol: "in-process/v1".into(),
            capabilities: AdapterCapabilities {
                languages: vec!["rust".into()],
                component_kinds: vec!["ui".into()],
                targets: vec!["windows".into()],
                operations: vec!["validate".into(), "compile".into()],
                requires: vec!["rustc".into()],
            },
            dev: None,
        },
    };
    BuildContext {
        project_root: root.to_path_buf(),
        target: "windows".into(),
        entry_path: Some(root.join("windows/app.yaml")),
        output,
        components: vec![component],
    }
}

#[test]
fn builds_and_runs_a_real_native_executable_with_escaped_content() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root, "fn main() {\n    println!(\"{{message}}\");\n}\n");
    let ctx = make_ctx(&root);

    let adapter = WindowsDesktopAdapter::new();

    let validate_diags = adapter.validate(&ctx);
    assert!(!validate_diags.has_errors(), "{:?}", validate_diags.items());

    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "source.rs"));
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path.starts_with("app/test-app")));
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "output.txt"));

    for artifact in &result.artifacts {
        assert!(
            artifact.absolute_path.is_file(),
            "artifact {} missing on disk",
            artifact.relative_path
        );
    }

    // The value is escaped for safe inclusion in a Rust string literal,
    // then printed verbatim at runtime -- so the *runtime* output is the
    // original, unescaped text (unlike HTML escaping, a Rust string
    // literal's escapes are consumed by the compiler, not visible at
    // runtime).
    let output = fs::read_to_string(root.join("dist/output.txt")).unwrap();
    assert_eq!(output.trim(), "Hi <there> & welcome");

    let source = fs::read_to_string(root.join("dist/source.rs")).unwrap();
    assert!(source.contains(r#""Hi <there> & welcome""#));
}

/// The crux of this adapter: a real Rust type error must fail both
/// `validate` and `compile` with a genuine `rustc` diagnostic -- proving
/// `validate()` actually invokes the compiler, not just the template
/// renderer (a real gap found and fixed while building this: template
/// rendering alone cannot catch this).
#[test]
fn a_genuine_compile_error_fails_both_validate_and_compile() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(
        &root,
        "fn main() {\n    let x: i32 = \"{{message}}\";\n    println!(\"{}\", x);\n}\n",
    );
    let ctx = make_ctx(&root);
    let adapter = WindowsDesktopAdapter::new();

    let validate_diags = adapter.validate(&ctx);
    assert!(validate_diags.has_errors());
    assert!(validate_diags
        .items()
        .iter()
        .any(|d| d.code == "E-WINDOWS-COMPILE-FAILED"));

    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-WINDOWS-COMPILE-FAILED"));
    assert!(result.artifacts.is_empty());
    assert!(!root.join("dist/output.txt").exists());
}

#[test]
fn missing_required_input_fails_the_build_without_partial_output() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root, "fn main() {\n    println!(\"{{message}}\");\n}\n");
    // Remove the only source of the required 'message' input.
    fs::write(
        root.join("windows/app.yaml"),
        r#"
schema: omniforge.dev/windows-app/v1
app_name: test-app
root:
  component: ui.greeting
"#,
    )
    .unwrap();
    let ctx = make_ctx(&root);

    let adapter = WindowsDesktopAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-WINDOWS-MISSING-INPUT"));
    assert!(result.artifacts.is_empty());
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = WindowsDesktopAdapter::new();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}
