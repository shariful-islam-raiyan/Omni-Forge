//! Black-box tests for `omni seo check`/`omni seo report` against the real
//! compiled CLI and real build output (not the analyzer function
//! in isolation) -- proves the command actually finds and reads
//! `dist/<target>/index.html`, not just that `commands::seo::analyze`
//! works on a hand-crafted string.

use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn bin() -> Command {
    Command::cargo_bin("omni").unwrap()
}

fn scaffold(root: &Path, seo_block: &str, template: &str) {
    fs::create_dir_all(root.join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(
        root.join("omni.yaml"),
        r#"schema: omniforge.dev/project/v1

project:
  name: seo-test
  version: 0.1.0

targets:
  web:
    enabled: true
    adapter: web-static
    entry: web/app.yaml
  android: { enabled: false }
  ios: { enabled: false }
  windows: { enabled: false }
  linux: { enabled: false }
  macos: { enabled: false }
  backend: { enabled: false }

components:
  directory: lib/components

build:
  output: dist
  cache: false
  parallel: false

security:
  strict_adapter_permissions: true
  audit_dependencies: true
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/ui.home/component.yaml"),
        r#"schema: omniforge.dev/component/v1
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
    fs::write(root.join("lib/components/ui.home/home.html.tmpl"), template).unwrap();
    fs::write(
        root.join("web/app.yaml"),
        format!(
            "schema: omniforge.dev/webapp/v1\ntitle: \"SEO test page\"\nroot:\n  component: ui.home\n  inputs:\n    title: \"Hi\"\n{seo_block}"
        ),
    )
    .unwrap();
}

#[test]
fn seo_check_passes_on_a_well_formed_page() {
    let dir = tempfile::tempdir().unwrap();
    scaffold(
        dir.path(),
        "seo:\n  description: \"a real description\"\n  site_url: \"https://example.com\"\n",
        "<h1>{{title}}</h1>",
    );
    bin()
        .current_dir(dir.path())
        .args(["build", "web"])
        .assert()
        .success();

    let assert = bin()
        .current_dir(dir.path())
        .args(["seo", "check", "web"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("All checks passed."), "{stdout}");
    assert!(stdout.contains("[PASS] title"), "{stdout}");
}

#[test]
fn seo_check_fails_on_a_page_missing_description_and_h1() {
    let dir = tempfile::tempdir().unwrap();
    // No seo: block at all, and the template has no <h1>.
    scaffold(dir.path(), "", "<p>{{title}}</p>");
    bin()
        .current_dir(dir.path())
        .args(["build", "web"])
        .assert()
        .success();

    let assert = bin()
        .current_dir(dir.path())
        .args(["seo", "check", "web"])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("[FAIL] meta-description"), "{stdout}");
    assert!(stdout.contains("[FAIL] single-h1"), "{stdout}");
    assert!(stdout.contains("[FAIL] canonical-url"), "{stdout}");
}

#[test]
fn seo_report_always_exits_zero_even_with_failing_checks() {
    let dir = tempfile::tempdir().unwrap();
    scaffold(dir.path(), "", "<p>{{title}}</p>");
    bin()
        .current_dir(dir.path())
        .args(["build", "web"])
        .assert()
        .success();

    bin()
        .current_dir(dir.path())
        .args(["seo", "report", "web"])
        .assert()
        .success();
}

#[test]
fn seo_check_reports_a_clear_error_when_the_target_was_never_built() {
    let dir = tempfile::tempdir().unwrap();
    scaffold(
        dir.path(),
        "seo:\n  description: \"unused\"\n",
        "<h1>{{title}}</h1>",
    );
    // Deliberately skip `omni build web`.
    let assert = bin()
        .current_dir(dir.path())
        .args(["seo", "check", "web"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(stderr.contains("E-SEO-NOT-BUILT"), "{stderr}");
}

#[test]
fn seo_check_json_output_is_valid_and_reflects_real_findings() {
    let dir = tempfile::tempdir().unwrap();
    scaffold(
        dir.path(),
        "seo:\n  description: \"a real description\"\n  site_url: \"https://example.com\"\n",
        "<h1>{{title}}</h1>",
    );
    bin()
        .current_dir(dir.path())
        .args(["build", "web"])
        .assert()
        .success();

    let assert = bin()
        .current_dir(dir.path())
        .args(["seo", "check", "web", "--json"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["target"], "web");
    assert_eq!(parsed["all_passed"], true);
    assert!(parsed["findings"].as_array().unwrap().len() >= 6);
}
