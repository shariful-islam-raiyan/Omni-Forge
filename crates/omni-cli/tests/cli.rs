//! End-to-end CLI tests: drive the real `omni` binary against temp
//! directories, exactly as a user would from a shell.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn bin() -> Command {
    Command::cargo_bin("omni").unwrap()
}

#[test]
fn version_and_help_work() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("omni"));
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OmniForge"));
}

#[test]
fn create_check_build_clean_round_trip() {
    let dir = tempfile::tempdir().unwrap();

    bin()
        .current_dir(dir.path())
        .args(["create", "demo-site"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created new OmniForge project"));

    let project_dir = dir.path().join("demo-site");
    assert!(project_dir.join("omni.yaml").is_file());

    bin()
        .current_dir(&project_dir)
        .arg("check")
        .assert()
        .success();

    bin()
        .current_dir(&project_dir)
        .args(["build", "web"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SUCCESS"));

    let index = project_dir.join("dist/web/index.html");
    assert!(index.is_file());
    let html = fs::read_to_string(&index).unwrap();
    assert!(html.contains("<h1>"));
    assert!(project_dir.join("dist/web/assets/style.css").is_file());
    assert!(project_dir.join("dist/build-report.json").is_file());

    // Rebuild should hit cache.
    bin()
        .current_dir(&project_dir)
        .args(["build", "web"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cache hit"));

    bin()
        .current_dir(&project_dir)
        .arg("clean")
        .assert()
        .success();
    assert!(!project_dir.join("dist").exists());
}

#[test]
fn create_rejects_invalid_name() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .args(["create", "Not_Valid!"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-CREATE-INVALID-NAME"));
}

#[test]
fn create_refuses_to_overwrite_nonempty_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("taken")).unwrap();
    fs::write(dir.path().join("taken/existing.txt"), "keep me").unwrap();

    bin()
        .current_dir(dir.path())
        .args(["create", "taken"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-CREATE-DIR-NOT-EMPTY"));

    assert_eq!(
        fs::read_to_string(dir.path().join("taken/existing.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn check_fails_on_invalid_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("omni.yaml"),
        "schema: omniforge.dev/project/v2\nproject:\n  name: x\n  version: 0.1.0\ntargets:\n  web:\n    enabled: false\n",
    )
    .unwrap();

    bin()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-SCHEMA-PROJECT"));
}

#[test]
fn check_fails_when_no_project_found() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-PROJECT-NOT-FOUND"));
}

#[test]
fn build_fails_cleanly_when_required_input_missing() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .args(["create", "broken"])
        .assert()
        .success();
    let project_dir = dir.path().join("broken");

    // 'title' is required; blank it out of app.yaml so it's neither provided
    // nor defaulted.
    fs::write(
        project_dir.join("web/app.yaml"),
        "schema: omniforge.dev/webapp/v1\ntitle: \"x\"\nroot:\n  component: ui.home\n",
    )
    .unwrap();

    bin()
        .current_dir(&project_dir)
        .args(["build", "web"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-WEB-MISSING-INPUT"));
    assert!(!project_dir.join("dist/web/index.html").exists());
}

#[test]
fn build_rejects_unknown_target_name() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .args(["create", "demo2"])
        .assert()
        .success();
    let project_dir = dir.path().join("demo2");

    bin()
        .current_dir(&project_dir)
        .args(["build", "quantum"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-BUILD-UNKNOWN-TARGET"));
}

#[test]
fn adapter_list_and_doctor_run_without_a_project() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .args(["adapter", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("web-static"));
    bin()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        .success();
}

#[test]
fn duplicate_component_names_fail_check() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .current_dir(dir.path())
        .args(["create", "dupes"])
        .assert()
        .success();
    let project_dir = dir.path().join("dupes");

    fs::create_dir_all(project_dir.join("lib/components/ui.home2")).unwrap();
    fs::copy(
        project_dir.join("lib/components/ui.home/component.yaml"),
        project_dir.join("lib/components/ui.home2/component.yaml"),
    )
    .unwrap();
    fs::copy(
        project_dir.join("lib/components/ui.home/home.html.tmpl"),
        project_dir.join("lib/components/ui.home2/home.html.tmpl"),
    )
    .unwrap();

    bin()
        .current_dir(&project_dir)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("E-COMPONENT-DUPLICATE"));
}

/// Full black-box proof that the out-of-process `python-backend` adapter
/// works end to end through the real compiled `omni` binary: discovery,
/// resolution, adapter invocation, real script execution, and artifact
/// verification — mirroring `create_check_build_clean_round_trip`'s web
/// coverage, but for the backend target.
#[test]
fn build_backend_target_executes_real_python_script_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("hello-backend");
    fs::create_dir_all(project_dir.join("lib/components/svc.greeter")).unwrap();
    fs::create_dir_all(project_dir.join("backend")).unwrap();

    fs::write(
        project_dir.join("omni.yaml"),
        "schema: omniforge.dev/project/v1\nproject:\n  name: hello-backend\n  version: 0.1.0\ntargets:\n  web:\n    enabled: false\n  backend:\n    enabled: true\n    adapter: python-backend\n    entry: backend/service.json\ncomponents:\n  directory: lib/components\n",
    )
    .unwrap();
    fs::write(
        project_dir.join("lib/components/svc.greeter/component.yaml"),
        "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs:\n    greeting:\n      type: string\n      required: false\n      default: \"default greeting\"\nimplementations:\n  backend:\n    adapter: python-backend\n    source: ./service.py\n",
    )
    .unwrap();
    fs::write(
        project_dir.join("lib/components/svc.greeter/service.py"),
        "import json, sys\nconfig = json.load(open(sys.argv[1])) if len(sys.argv) > 1 else {}\nprint(config.get('greeting', 'default'))\n",
    )
    .unwrap();
    fs::write(
        project_dir.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {\"greeting\": \"hello from the CLI test\"}}",
    )
    .unwrap();

    bin()
        .current_dir(&project_dir)
        .arg("check")
        .assert()
        .success();

    bin()
        .current_dir(&project_dir)
        .args(["build", "backend"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SUCCESS"));

    let output_txt = project_dir.join("dist/backend/output.txt");
    assert!(output_txt.is_file());
    assert_eq!(
        fs::read_to_string(&output_txt).unwrap().trim(),
        "hello from the CLI test"
    );
    assert!(project_dir.join("dist/backend/app/service.py").is_file());
    assert!(project_dir.join("dist/backend/run.sh").is_file());
    assert!(project_dir.join("dist/backend/run.bat").is_file());
    assert!(project_dir.join("dist/build-report.json").is_file());

    bin()
        .current_dir(&project_dir)
        .arg("clean")
        .assert()
        .success();
    assert!(!project_dir.join("dist").exists());
}
