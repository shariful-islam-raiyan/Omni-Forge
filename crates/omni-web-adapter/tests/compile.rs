//! End-to-end exercise of the web-static adapter against a synthetic
//! project tree, independent of the CLI. This is what proves the adapter
//! itself (not just its template renderer) produces real, verifiable
//! output.

use omni_adapter_api::{Adapter, BuildContext};
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::{AdapterCapabilities, AdapterManifest};
use omni_core::spec::component::ComponentImplementation;
use omni_web_adapter::WebStaticAdapter;
use std::fs;
use tempfile::tempdir;

fn scaffold(root: &std::path::Path) {
    fs::create_dir_all(root.join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::create_dir_all(root.join("public")).unwrap();

    fs::write(
        root.join("lib/components/ui.home/component.yaml"),
        r#"
schema: omniforge.dev/component/v1
name: ui.home
version: 1.0.0
kind: ui
interface:
  inputs:
    title:
      type: string
      required: true
    message:
      type: string
      required: false
      default: "default message"
implementations:
  web:
    adapter: web-static
    source: ./home.html.tmpl
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.home/home.html.tmpl"),
        "<h1>{{title}}</h1>\n<p>{{message}}</p>",
    )
    .unwrap();

    fs::write(
        root.join("web/app.yaml"),
        r#"
schema: omniforge.dev/webapp/v1
title: "Hello OmniForge"
root:
  component: ui.home
  inputs:
    title: "Hi <there> & welcome"
public:
  css: ["style.css"]
"#,
    )
    .unwrap();
    fs::write(root.join("public/style.css"), "body { color: navy; }").unwrap();
}

fn make_ctx(root: &std::path::Path) -> BuildContext {
    let output = OutputDir::resolve(root, "dist").unwrap();
    let component = ResolvedComponent {
        name: "ui.home".into(),
        version: "1.0.0".into(),
        kind: omni_core::spec::component::ComponentKind::Ui,
        implementation: ComponentImplementation {
            adapter: "web-static".into(),
            source: "./home.html.tmpl".into(),
        },
        manifest_dir: root.join("lib/components/ui.home"),
        adapter: AdapterManifest {
            schema: "omniforge.dev/adapter/v1".into(),
            id: "web-static".into(),
            version: "0.1.0".into(),
            protocol: "in-process/v1".into(),
            capabilities: AdapterCapabilities {
                languages: vec!["template".into()],
                component_kinds: vec!["ui".into()],
                targets: vec!["web".into()],
                operations: vec!["validate".into(), "compile".into(), "package".into()],
                requires: vec![],
            },
            dev: None,
        },
    };
    BuildContext {
        project_root: root.to_path_buf(),
        target: "web".into(),
        entry_path: Some(root.join("web/app.yaml")),
        output,
        components: vec![component],
    }
}

#[test]
fn builds_real_site_with_escaped_content_and_verified_assets() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root);
    let ctx = make_ctx(&root);

    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );
    assert!(result.artifacts.len() >= 3);

    let html = fs::read_to_string(root.join("dist/index.html")).unwrap();
    assert!(html.contains("Hi &lt;there&gt; &amp; welcome"));
    assert!(html.contains("default message"));
    assert!(html.contains("assets/style.css"));
    assert!(
        !html.contains("<there>"),
        "unescaped user content leaked into output"
    );

    assert!(root.join("dist/assets/style.css").is_file());
    assert_eq!(
        fs::read_to_string(root.join("dist/assets/style.css")).unwrap(),
        "body { color: navy; }"
    );

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("dist/build-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["adapter"]["id"], "web-static");
}

#[test]
fn missing_required_input_fails_the_build_without_partial_output() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root);
    // Remove the only source of the required 'title' input.
    fs::write(
        root.join("web/app.yaml"),
        r#"
schema: omniforge.dev/webapp/v1
title: "Hello OmniForge"
root:
  component: ui.home
"#,
    )
    .unwrap();
    let ctx = make_ctx(&root);

    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result.artifacts.is_empty());
    assert!(!root.join("dist/index.html").exists());
}

#[test]
fn missing_public_asset_fails_the_build() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold(&root);
    fs::remove_file(root.join("public/style.css")).unwrap();
    let ctx = make_ctx(&root);

    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-WEB-ASSET-MISSING"));
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = WebStaticAdapter::new();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}
