//! White-box tests for the `python-backend` adapter's own business logic
//! (input merging, path safety, running the user's script, artifact
//! writing) — as opposed to `omni-adapter-api`'s `process` module tests,
//! which cover the generic out-of-process *mechanism* (timeouts, malformed
//! responses, protocol mismatches) independent of any particular adapter.
//!
//! These drive the real, embedded `adapters/python-backend/adapter.py`
//! directly via `omni_adapter_api::process::execute`/`ProcessAdapter`, not
//! through the compiled `omni` binary — this crate exposes a `lib` target
//! (`omni_cli`) specifically so tests can do this without relying on
//! environment-dependent tricks (e.g. clearing `PATH`) to simulate failure
//! modes like a missing toolchain.

use omni_adapter_api::process::{execute, resolve_executable};
use omni_adapter_api::protocol::{AdapterRequest, Operation, PROTOCOL_VERSION};
use omni_adapter_api::{Adapter, BuildContext};
use omni_cli::adapters::python_backend;
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::{AdapterCapabilities, AdapterManifest};
use omni_core::spec::component::{ComponentImplementation, ComponentKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn python_candidates() -> Vec<String> {
    vec![
        "python3".to_string(),
        "python".to_string(),
        "py".to_string(),
    ]
}

fn scaffold_greeter(root: &Path, greeting: &str, required: bool) {
    let comp_dir = root.join("lib/components/svc.greeter");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();

    fs::write(
        comp_dir.join("component.yaml"),
        format!(
            "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs:\n    greeting:\n      type: string\n      required: {required}\n      default: \"{greeting}\"\nimplementations:\n  backend:\n    adapter: python-backend\n    source: ./service.py\n"
        ),
    )
    .unwrap();

    fs::write(
        comp_dir.join("service.py"),
        "import json, sys\nconfig = json.load(open(sys.argv[1])) if len(sys.argv) > 1 else {}\nprint(config.get('greeting', 'default'))\n",
    )
    .unwrap();

    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {}}",
    )
    .unwrap();
}

fn adapter_manifest() -> AdapterManifest {
    AdapterManifest {
        schema: "omniforge.dev/adapter/v1".into(),
        id: "python-backend".into(),
        version: "0.1.0".into(),
        protocol: "out-of-process/v1".into(),
        capabilities: AdapterCapabilities {
            languages: vec!["python".into()],
            component_kinds: vec!["service".into()],
            targets: vec!["backend".into()],
            operations: vec!["validate".into(), "compile".into()],
            requires: vec!["python3".into()],
        },
        dev: None,
    }
}

fn make_ctx(root: &Path) -> BuildContext {
    let output = OutputDir::resolve(root, "dist/backend").unwrap();
    let component = ResolvedComponent {
        name: "svc.greeter".into(),
        version: "1.0.0".into(),
        kind: ComponentKind::Service,
        implementation: ComponentImplementation {
            adapter: "python-backend".into(),
            source: "./service.py".into(),
        },
        manifest_dir: root.join("lib/components/svc.greeter"),
        adapter: adapter_manifest(),
    };
    BuildContext {
        project_root: root.to_path_buf(),
        target: "backend".into(),
        entry_path: Some(root.join("backend/service.json")),
        output,
        components: vec![component],
    }
}

#[test]
fn full_round_trip_executes_the_real_script_and_produces_verified_artifacts() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_greeter(&root, "hello from the white-box test", true);
    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {\"greeting\": \"white-box greeting\"}}",
    )
    .unwrap();

    let ctx = make_ctx(&root);
    let adapter = python_backend::build();

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
        .any(|a| a.relative_path == "output.txt"));

    for artifact in &result.artifacts {
        assert!(
            artifact.absolute_path.is_file(),
            "artifact {} missing on disk",
            artifact.relative_path
        );
    }

    let output = fs::read_to_string(root.join("dist/backend/output.txt")).unwrap();
    assert_eq!(output.trim(), "white-box greeting");
}

#[test]
fn missing_toolchain_is_reported_without_panicking() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_greeter(&root, "unused", true);
    let ctx = make_ctx(&root);

    let adapter = python_backend::build_with_candidates(vec![
        "definitely-not-a-real-python-xyz123".to_string(),
    ]);
    let diags = adapter.validate(&ctx);
    assert!(diags.has_errors());
    assert!(diags
        .items()
        .iter()
        .any(|d| d.code == "E-ADAPTER-TOOLCHAIN-MISSING"));
}

#[test]
fn missing_required_input_fails_cleanly_with_no_artifacts() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let comp_dir = root.join("lib/components/svc.greeter");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();
    // required: true, and deliberately no `default:` key at all, so
    // nothing can satisfy the requirement.
    fs::write(
        comp_dir.join("component.yaml"),
        "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs:\n    greeting:\n      type: string\n      required: true\nimplementations:\n  backend:\n    adapter: python-backend\n    source: ./service.py\n",
    )
    .unwrap();
    fs::write(comp_dir.join("service.py"), "print('should never run')\n").unwrap();
    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {}}",
    )
    .unwrap();
    let ctx = make_ctx(&root);

    let adapter = python_backend::build();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-PYADAPTER-MISSING-INPUT"));
    assert!(result.artifacts.is_empty());
    assert!(!root.join("dist/backend/output.txt").exists());
}

fn build_raw_request(
    project_root: &Path,
    output_dir: &Path,
    entry_path: Option<&Path>,
    components: serde_json::Value,
    operation: Operation,
    timeout_ms: u64,
) -> AdapterRequest {
    let mut config = BTreeMap::new();
    config.insert("components".to_string(), components);
    AdapterRequest {
        protocol: PROTOCOL_VERSION.to_string(),
        request_id: "white-box-test".to_string(),
        operation,
        project_root: project_root.display().to_string(),
        target: "backend".to_string(),
        component: None,
        source_files: vec![],
        entry_path: entry_path.map(|p| p.display().to_string()),
        output_dir: output_dir.display().to_string(),
        config,
        timeout_ms,
    }
}

/// Materializes the real embedded adapter script inside `workdir` (expected
/// to be a tempdir the caller already owns and keeps alive for the test's
/// duration) and resolves a real Python launcher to run it with.
fn real_adapter_invocation(workdir: &Path) -> (String, std::path::PathBuf) {
    let launcher =
        resolve_executable(&python_candidates()).expect("a usable Python must exist for this test");
    let script_path = workdir.join(python_backend::script_filename());
    fs::write(&script_path, python_backend::script_source()).unwrap();
    (launcher, script_path)
}

#[test]
fn unsupported_operation_is_rejected_by_the_real_adapter() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (launcher, script_path) = real_adapter_invocation(&root);

    let request = build_raw_request(
        &root,
        &root,
        None,
        serde_json::json!([]),
        Operation::Package,
        5000,
    );
    let response = execute(
        &launcher,
        &script_path,
        &root,
        &request,
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(
        response.status,
        omni_adapter_api::protocol::ResponseStatus::Error
    );
    assert!(response
        .diagnostics
        .iter()
        .any(|d| d.code == "E-PYADAPTER-UNSUPPORTED-OPERATION"));
}

#[test]
fn unsafe_component_source_path_is_rejected() {
    let project_dir = tempdir().unwrap();
    let project_root = fs::canonicalize(project_dir.path()).unwrap();
    let (launcher, script_path) = real_adapter_invocation(&project_root);
    let outside_dir = tempdir().unwrap();
    let outside_root = fs::canonicalize(outside_dir.path()).unwrap();
    let evil_script = outside_root.join("evil.py");
    fs::write(&evil_script, "print('should never run')\n").unwrap();

    let components = serde_json::json!([{
        "name": "svc.evil",
        "version": "1.0.0",
        "kind": "service",
        "interface": {"inputs": {}, "outputs": {}, "events": {}},
        "source_path": evil_script.display().to_string(),
    }]);
    let request = build_raw_request(
        &project_root,
        &project_root,
        None,
        components,
        Operation::Validate,
        5000,
    );
    let response = execute(
        &launcher,
        &script_path,
        &project_root,
        &request,
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(
        response.status,
        omni_adapter_api::protocol::ResponseStatus::Error
    );
    assert!(response
        .diagnostics
        .iter()
        .any(|d| d.code == "E-PYADAPTER-UNSAFE-PATH"));
}

#[test]
fn backend_script_nonzero_exit_is_reported_as_execution_failure() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (launcher, script_path) = real_adapter_invocation(&root);
    let failing_script = root.join("failing.py");
    fs::write(&failing_script, "import sys\nsys.exit(5)\n").unwrap();

    let components = serde_json::json!([{
        "name": "svc.failing",
        "version": "1.0.0",
        "kind": "service",
        "interface": {"inputs": {}, "outputs": {}, "events": {}},
        "source_path": failing_script.display().to_string(),
    }]);
    let request = build_raw_request(&root, &root, None, components, Operation::Compile, 10000);
    let response = execute(
        &launcher,
        &script_path,
        &root,
        &request,
        Duration::from_secs(15),
    )
    .unwrap();
    assert_eq!(
        response.status,
        omni_adapter_api::protocol::ResponseStatus::Error
    );
    assert!(response
        .diagnostics
        .iter()
        .any(|d| d.code == "E-PYADAPTER-EXECUTION-FAILED"));
    assert!(response.artifacts.is_empty());
}

#[test]
fn backend_script_inner_timeout_is_reported_without_hanging() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (launcher, script_path) = real_adapter_invocation(&root);
    let slow_script = root.join("slow.py");
    fs::write(&slow_script, "import time\ntime.sleep(60)\n").unwrap();

    let components = serde_json::json!([{
        "name": "svc.slow",
        "version": "1.0.0",
        "kind": "service",
        "interface": {"inputs": {}, "outputs": {}, "events": {}},
        "source_path": slow_script.display().to_string(),
    }]);
    // timeout_ms is small enough that the adapter's *own* inner
    // subprocess.run(timeout=...) fires well before the outer Rust-side
    // `execute()` timeout (10s) would.
    let request = build_raw_request(&root, &root, None, components, Operation::Compile, 1500);
    let start = std::time::Instant::now();
    let response = execute(
        &launcher,
        &script_path,
        &root,
        &request,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "inner timeout should fire well under the outer budget"
    );
    assert_eq!(
        response.status,
        omni_adapter_api::protocol::ResponseStatus::Error
    );
    assert!(response
        .diagnostics
        .iter()
        .any(|d| d.code == "E-PYADAPTER-EXECUTION-TIMEOUT"));
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = python_backend::build();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}
