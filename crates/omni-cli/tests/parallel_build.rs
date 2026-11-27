//! Proves `build.parallel: true` in `omni.yaml` actually builds independent
//! targets concurrently (one OS thread per target, via `std::thread::scope`
//! in `crates/omni-cli/src/commands/build.rs`) while producing results
//! identical to the sequential path: correct per-target artifacts, correct
//! failure isolation (one target failing must not affect another), and a
//! deterministic `build-report.json` regardless of which target's thread
//! actually finishes first.

use assert_cmd::Command;
use omni_core::spec::build_report::{BuildReport, TargetStatus};
use std::fs;
use std::path::Path;

fn bin() -> Command {
    Command::cargo_bin("omni").unwrap()
}

fn node_available() -> bool {
    omni_adapter_api::process::resolve_executable(&omni_adapter_api::process::known_aliases("node"))
        .is_some()
}

/// `web` (real, working `web-static`) + `backend` (real, working
/// `node-backend`) in one project, `build.parallel: true`. Both targets are
/// independent (no shared components) and both must genuinely succeed when
/// built concurrently.
fn scaffold_parallel_success_project(root: &Path) {
    fs::create_dir_all(root.join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(root.join("lib/components/svc.greeter")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();

    fs::write(
        root.join("omni.yaml"),
        r#"schema: omniforge.dev/project/v1

project:
  name: parallel-success
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
    adapter: node-backend
    entry: backend/service.json

components:
  directory: lib/components

build:
  output: dist
  cache: false
  parallel: true

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
title: "Parallel build test"
root:
  component: ui.home
  inputs:
    title: "Hello from parallel web"
"#,
    )
    .unwrap();

    fs::write(
        root.join("lib/components/svc.greeter/component.yaml"),
        r#"schema: omniforge.dev/component/v1
name: svc.greeter
version: 1.0.0
kind: service
interface:
  inputs:
    greeting:
      type: string
      required: false
      default: "default greeting"
implementations:
  backend:
    adapter: node-backend
    source: ./service.js
"#,
    )
    .unwrap();
    fs::write(
        root.join("lib/components/svc.greeter/service.js"),
        "const fs = require('fs');\nconst config = process.argv.length > 2 ? JSON.parse(fs.readFileSync(process.argv[2], 'utf8')) : {};\nconsole.log(config.greeting || 'default');\n",
    )
    .unwrap();
    fs::write(
        root.join("backend/service.json"),
        r#"{"schema": "omniforge.dev/backend-app/v1", "service": "svc.greeter", "inputs": {"greeting": "Hello from parallel backend"}}"#,
    )
    .unwrap();
}

#[test]
fn parallel_build_runs_two_independent_targets_concurrently_and_both_succeed() {
    if !node_available() {
        eprintln!("skipping: no usable Node.js on this machine");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    scaffold_parallel_success_project(dir.path());

    let assert = bin()
        .current_dir(dir.path())
        .args(["build", "all"])
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("building 2 targets in parallel"),
        "{stdout}"
    );
    assert!(stdout.contains("target 'web': SUCCESS"), "{stdout}");
    assert!(stdout.contains("target 'backend': SUCCESS"), "{stdout}");

    let web_html = fs::read_to_string(dir.path().join("dist/web/index.html")).unwrap();
    assert!(web_html.contains("Hello from parallel web"));

    let backend_output = fs::read_to_string(dir.path().join("dist/backend/output.txt")).unwrap();
    assert_eq!(backend_output.trim(), "Hello from parallel backend");

    let report_raw = fs::read_to_string(dir.path().join("dist/build-report.json")).unwrap();
    let report: BuildReport = serde_json::from_str(&report_raw).unwrap();
    assert_eq!(report.targets.len(), 2);
    assert!(report
        .targets
        .iter()
        .all(|t| t.status == TargetStatus::Success));
}

/// Same shape as `build_all_partial_failure.rs`'s scenario, but with
/// `build.parallel: true` -- proves the parallel path preserves correct
/// failure isolation: one target's failure must not affect the other's
/// already-written artifacts or status, even when both were built on
/// separate threads. Needs no external toolchain: `backend` fails at
/// resolution (`E-BUILD-NO-COMPONENTS`) before any adapter process would be
/// spawned.
#[test]
fn parallel_build_isolates_one_targets_failure_from_the_other() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("lib/components/ui.home")).unwrap();
    fs::create_dir_all(dir.path().join("web")).unwrap();

    fs::write(
        dir.path().join("omni.yaml"),
        r#"schema: omniforge.dev/project/v1

project:
  name: parallel-partial-failure
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
  cache: false
  parallel: true

security:
  strict_adapter_permissions: true
  audit_dependencies: true
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("lib/components/ui.home/component.yaml"),
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
        dir.path().join("lib/components/ui.home/home.html.tmpl"),
        "<h1>{{title}}</h1>\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("web/app.yaml"),
        r#"schema: omniforge.dev/webapp/v1
title: "Mixed project"
root:
  component: ui.home
  inputs:
    title: "Hello"
"#,
    )
    .unwrap();
    // Deliberately no `backend/` component -- `backend` has zero components
    // implementing it, a deterministic pre-adapter resolution failure.

    let assert = bin()
        .current_dir(dir.path())
        .args(["build", "all"])
        .assert()
        .failure();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("target 'web': SUCCESS"), "{stdout}");
    assert!(stdout.contains("target 'backend': FAILED"), "{stdout}");
    assert!(
        dir.path().join("dist/web/index.html").is_file(),
        "web's artifacts must survive backend's failure under parallel scheduling too"
    );
}
