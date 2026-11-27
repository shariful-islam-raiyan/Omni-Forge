//! Black-box tests for `omni adapter validate` against the real compiled
//! CLI -- validates a manifest **on its own**, with no registration or
//! execution involved (see `crates/omni-cli/src/commands/adapter_validate.rs`).

use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;

fn bin() -> Command {
    Command::cargo_bin("omni").unwrap()
}

/// `CARGO_MANIFEST_DIR` is this crate's own directory (`crates/omni-cli`),
/// not the workspace root where `adapters/` actually lives.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn validates_a_real_shipped_adapter_manifest_as_a_directory() {
    // node-backend's own real, shipped manifest -- proves this command
    // genuinely re-validates manifests that are also embedded/compiled
    // into this binary, not just hand-crafted fixtures.
    bin()
        .args(["adapter", "validate", "adapters/node-backend"])
        .current_dir(workspace_root())
        .assert()
        .success()
        .stdout(predicate_contains("PASSED"));
}

#[test]
fn validates_a_manifest_file_path_directly() {
    bin()
        .args(["adapter", "validate", "adapters/go-backend/adapter.yaml"])
        .current_dir(workspace_root())
        .assert()
        .success();
}

#[test]
fn rejects_a_schema_invalid_manifest() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("adapter.yaml"),
        r#"schema: omniforge.dev/adapter/v1
id: Bad_ID_With_Caps
version: not-semver
protocol: made-up-protocol
capabilities:
  languages: []
  component_kinds: []
  targets: []
  operations: []
"#,
    )
    .unwrap();

    let assert = bin()
        .args(["adapter", "validate", dir.path().to_str().unwrap()])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("FAILED schema validation"), "{stdout}");
    assert!(stdout.contains("does not match"), "{stdout}");
}

#[test]
fn rejects_a_schema_valid_but_non_conformant_manifest() {
    // Passes JSON Schema (an empty `requires` list is schema-legal) but
    // fails a conformance invariant: declaring zero targets.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("adapter.yaml"),
        r#"schema: omniforge.dev/adapter/v1
id: no-targets-adapter
version: 0.1.0
protocol: in-process/v1
capabilities:
  languages: []
  component_kinds: ["service"]
  targets: []
  operations: ["validate"]
"#,
    )
    .unwrap();

    let assert = bin()
        .args(["adapter", "validate", dir.path().to_str().unwrap()])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(stdout.contains("FAILED conformance"), "{stdout}");
    assert!(stdout.contains("targets"), "{stdout}");
}

#[test]
fn reports_a_clear_error_for_a_missing_path() {
    bin()
        .args(["adapter", "validate", "/definitely/not/a/real/path"])
        .assert()
        .failure()
        .stderr(predicate_contains("E-ADAPTER-VALIDATE-NOT-FOUND"));
}

#[test]
fn json_output_is_valid_and_reflects_real_findings() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("adapter.yaml"),
        r#"schema: omniforge.dev/adapter/v1
id: no-targets-adapter
version: 0.1.0
protocol: in-process/v1
capabilities:
  languages: []
  component_kinds: ["service"]
  targets: []
  operations: ["validate"]
"#,
    )
    .unwrap();

    let assert = bin()
        .args([
            "adapter",
            "validate",
            dir.path().to_str().unwrap(),
            "--json",
        ])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["passed"], false);
    assert!(!parsed["conformance_violations"]
        .as_array()
        .unwrap()
        .is_empty());
}

fn predicate_contains(s: &str) -> predicates::str::ContainsPredicate {
    predicates::str::contains(s.to_string())
}
