//! Proves `omni build all` reports each enabled target's status
//! independently: one target failing must not erase another target's
//! already-written artifacts, and the overall exit code must reflect the
//! failure. The orchestrator code path this exercises
//! (`crates/omni-cli/src/commands/build.rs`) already loops over every
//! enabled target and computes `any_failed`/`any_unsupported` from each
//! target's own `TargetReport` — this test exists to prove that behavior,
//! not to add new orchestrator logic.

use assert_cmd::Command;
use omni_core::spec::build_report::{BuildReport, TargetStatus};
use std::fs;
use std::path::Path;

fn bin() -> Command {
    Command::cargo_bin("omni").unwrap()
}

/// A project with `web` fully working and `backend` enabled but with no
/// component declaring a `backend` implementation at all — a deterministic
/// failure (`E-BUILD-NO-COMPONENTS`) that requires no Python toolchain,
/// since the resolver/orchestrator rejects it before ever invoking an
/// adapter.
fn scaffold_mixed_project(root: &Path) {
    fs::create_dir_all(root.join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();

    fs::write(
        root.join("omni.yaml"),
        r#"schema: omniforge.dev/project/v1

project:
  name: mixed-project
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
  backend:
    enabled: true
    adapter: python-backend
    entry: backend/service.json

components:
  directory: lib/components

build:
  output: dist
  cache: true
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

    fs::write(
        root.join("lib/components/ui.home/home.html.tmpl"),
        "<h1>{{title}}</h1>\n",
    )
    .unwrap();

    fs::write(
        root.join("web/app.yaml"),
        r#"schema: omniforge.dev/webapp/v1
title: "Mixed project"
root:
  component: ui.home
  inputs:
    title: "Hello"
"#,
    )
    .unwrap();

    // Deliberately no `backend/` component and no `backend/service.json` —
    // the `backend` target has zero components implementing it.
}

#[test]
fn build_all_reports_each_target_independently_on_partial_failure() {
    let dir = tempfile::tempdir().unwrap();
    scaffold_mixed_project(dir.path());

    let assert = bin()
        .current_dir(dir.path())
        .args(["build", "all"])
        .assert()
        .failure();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("target 'web': SUCCESS"), "{stdout}");
    assert!(stdout.contains("target 'backend': FAILED"), "{stdout}");

    // The web target's artifacts must survive the backend target's failure.
    assert!(
        dir.path().join("dist/web/index.html").is_file(),
        "web's artifacts must not be erased by backend's failure"
    );

    let report_raw = fs::read_to_string(dir.path().join("dist/build-report.json"))
        .expect("build-report.json is written even on partial failure");
    let report: BuildReport = serde_json::from_str(&report_raw).unwrap();

    let web = report
        .targets
        .iter()
        .find(|t| t.target == "web")
        .expect("web target reported");
    assert_eq!(web.status, TargetStatus::Success);
    assert!(!web.artifacts.is_empty());

    let backend = report
        .targets
        .iter()
        .find(|t| t.target == "backend")
        .expect("backend target reported");
    assert_eq!(backend.status, TargetStatus::Failed);
    assert!(backend
        .diagnostics
        .iter()
        .any(|d| d.code == "E-BUILD-NO-COMPONENTS"));
}
