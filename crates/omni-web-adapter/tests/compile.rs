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

fn scaffold_composition(root: &std::path::Path) {
    fs::create_dir_all(root.join("lib/components/ui.navbar")).unwrap();
    fs::create_dir_all(root.join("lib/components/ui.page")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::create_dir_all(root.join("public")).unwrap();

    fs::write(
        root.join("lib/components/ui.navbar/component.yaml"),
        r#"
schema: omniforge.dev/component/v1
name: ui.navbar
version: 1.0.0
kind: ui
interface:
  inputs:
    brand:
      type: string
      required: true
implementations:
  web:
    adapter: web-static
    source: ./navbar.html.tmpl
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.navbar/navbar.html.tmpl"),
        "<nav>{{brand}}</nav>",
    )
    .unwrap();

    fs::write(
        root.join("lib/components/ui.page/component.yaml"),
        r#"
schema: omniforge.dev/component/v1
name: ui.page
version: 1.0.0
kind: ui
dependencies: [ui.navbar]
composes:
  nav:
    component: ui.navbar
    inputs:
      brand: "<script>evil</script>"
interface:
  inputs:
    heading:
      type: string
      required: true
implementations:
  web:
    adapter: web-static
    source: ./page.html.tmpl
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.page/page.html.tmpl"),
        "{{nav}}<h1>{{heading}}</h1>",
    )
    .unwrap();

    fs::write(
        root.join("web/app.yaml"),
        r#"
schema: omniforge.dev/webapp/v1
title: "Composition demo"
root:
  component: ui.page
  inputs:
    heading: "Welcome"
"#,
    )
    .unwrap();
}

fn make_composition_ctx(root: &std::path::Path) -> BuildContext {
    let output = OutputDir::resolve(root, "dist").unwrap();
    let adapter = AdapterManifest {
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
    };
    let page = ResolvedComponent {
        name: "ui.page".into(),
        version: "1.0.0".into(),
        kind: omni_core::spec::component::ComponentKind::Ui,
        implementation: ComponentImplementation {
            adapter: "web-static".into(),
            source: "./page.html.tmpl".into(),
        },
        manifest_dir: root.join("lib/components/ui.page"),
        adapter: adapter.clone(),
    };
    let navbar = ResolvedComponent {
        name: "ui.navbar".into(),
        version: "1.0.0".into(),
        kind: omni_core::spec::component::ComponentKind::Ui,
        implementation: ComponentImplementation {
            adapter: "web-static".into(),
            source: "./navbar.html.tmpl".into(),
        },
        manifest_dir: root.join("lib/components/ui.navbar"),
        adapter,
    };
    BuildContext {
        project_root: root.to_path_buf(),
        target: "web".into(),
        entry_path: Some(root.join("web/app.yaml")),
        output,
        components: vec![page, navbar],
    }
}

#[test]
fn composed_child_component_is_rendered_and_escaped_in_place() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_composition(&root);
    let ctx = make_composition_ctx(&root);

    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );

    let html = fs::read_to_string(root.join("dist/index.html")).unwrap();
    assert!(html.contains("<nav>&lt;script&gt;evil&lt;/script&gt;</nav>"));
    assert!(html.contains("<h1>Welcome</h1>"));
    assert!(!html.contains("<script>evil"));
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = WebStaticAdapter::new();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}

fn scaffold_with_seo(root: &std::path::Path, app_yaml_seo_block: &str) {
    fs::create_dir_all(root.join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
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
implementations:
  web:
    adapter: web-static
    source: ./home.html.tmpl
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.home/home.html.tmpl"),
        "<h1>{{title}}</h1>",
    )
    .unwrap();
    fs::write(
        root.join("web/app.yaml"),
        format!(
            "schema: omniforge.dev/webapp/v1\ntitle: \"Hello & <Welcome>\"\nroot:\n  component: ui.home\n  inputs:\n    title: \"Hi\"\n{app_yaml_seo_block}"
        ),
    )
    .unwrap();
}

#[test]
fn full_seo_config_produces_real_meta_tags_robots_and_sitemap() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_with_seo(
        &root,
        r#"seo:
  description: "A real & honest description"
  site_url: "https://example.com/"
  og_image: "https://example.com/og.png"
  robots: "index, follow"
  lang: "fr"
"#,
    );
    let ctx = make_ctx(&root);
    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "robots.txt"));
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "sitemap.xml"));

    let html = fs::read_to_string(root.join("dist/index.html")).unwrap();
    assert!(html.contains("<html lang=\"fr\">"));
    assert!(
        html.contains("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">")
    );
    assert!(
        html.contains("<meta name=\"description\" content=\"A real &amp; honest description\">")
    );
    assert!(html.contains("<meta name=\"robots\" content=\"index, follow\">"));
    assert!(html.contains("<link rel=\"canonical\" href=\"https://example.com/\">"));
    assert!(html.contains("<meta property=\"og:title\" content=\"Hello &amp; &lt;Welcome&gt;\">"));
    assert!(html.contains("<meta property=\"og:image\" content=\"https://example.com/og.png\">"));
    assert!(html.contains("\"@type\":\"WebSite\""));

    let robots = fs::read_to_string(root.join("dist/robots.txt")).unwrap();
    assert!(robots.contains("User-agent: *"));
    assert!(robots.contains("Allow: /"));
    assert!(robots.contains("Sitemap: https://example.com/sitemap.xml"));

    let sitemap = fs::read_to_string(root.join("dist/sitemap.xml")).unwrap();
    assert!(sitemap.contains("<loc>https://example.com/</loc>"));
}

#[test]
fn without_site_url_sitemap_is_honestly_skipped_but_robots_still_allows_all() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_with_seo(&root, "");
    let ctx = make_ctx(&root);
    let adapter = WebStaticAdapter::new();
    let result = adapter.compile(&ctx);
    assert!(
        !result.diagnostics.has_errors(),
        "{:?}",
        result.diagnostics.items()
    );
    assert!(!result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "sitemap.xml"));
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "robots.txt"));
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "I-WEB-SITEMAP-SKIPPED"));

    let robots = fs::read_to_string(root.join("dist/robots.txt")).unwrap();
    assert_eq!(robots, "User-agent: *\nAllow: /\n");

    // No seo config at all still gets the always-on defaults: viewport
    // and a default-permissive robots meta.
    let html = fs::read_to_string(root.join("dist/index.html")).unwrap();
    assert!(html.contains("<meta name=\"viewport\""));
    assert!(html.contains("<meta name=\"robots\" content=\"index, follow\">"));
    assert!(!html.contains("rel=\"canonical\""));
}
