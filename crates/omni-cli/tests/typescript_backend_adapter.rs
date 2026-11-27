//! White-box tests for the `typescript-backend` adapter -- mirrors
//! `node_backend_adapter.rs`'s structure, plus the one thing genuinely new
//! here: proving `tsc` type-checking is real, not merely type-stripping.
//! Drives the real, embedded `adapters/typescript-backend/adapter.js`
//! directly, against a real Node interpreter and a real TypeScript
//! compiler (resolved on demand via `npx` if not already installed -- see
//! docs/decisions/0013-typescript-backend-adapter.md).

use omni_adapter_api::process::{execute, resolve_executable};
use omni_adapter_api::protocol::{AdapterRequest, Operation, PROTOCOL_VERSION};
use omni_adapter_api::{Adapter, BuildContext};
use omni_cli::adapters::typescript_backend;
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::{AdapterCapabilities, AdapterManifest};
use omni_core::spec::component::{ComponentImplementation, ComponentKind};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn node_candidates() -> Vec<String> {
    vec!["node".to_string(), "nodejs".to_string()]
}

const VALID_SERVICE_TS: &str = "interface Config { greeting?: string }\nfunction main(): void {\n  let config: Config = {};\n  if (process.argv.length > 2) {\n    const fs = require('fs');\n    config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));\n  }\n  console.log(config.greeting ?? 'default');\n}\nmain();\n";

/// Deliberately assigns a string to a `number`-typed binding -- a real
/// type error (`TS2322`) that only genuine type-checking, not mere
/// `.ts`-to-`.js` transpilation, would ever catch.
const TYPE_BROKEN_SERVICE_TS: &str =
    "const brokenTypeError: number = \"this is a string, not a number\";\nconsole.log(brokenTypeError);\n";

fn scaffold_greeter(root: &Path, source_ts: &str, greeting: &str, required: bool) {
    let comp_dir = root.join("lib/components/svc.greeter");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();

    fs::write(
        comp_dir.join("component.yaml"),
        format!(
            "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs:\n    greeting:\n      type: string\n      required: {required}\n      default: \"{greeting}\"\nimplementations:\n  backend:\n    adapter: typescript-backend\n    source: ./service.ts\n"
        ),
    )
    .unwrap();

    fs::write(comp_dir.join("service.ts"), source_ts).unwrap();

    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {}}",
    )
    .unwrap();
}

fn adapter_manifest() -> AdapterManifest {
    AdapterManifest {
        schema: "omniforge.dev/adapter/v1".into(),
        id: "typescript-backend".into(),
        version: "0.1.0".into(),
        protocol: "out-of-process/v1".into(),
        capabilities: AdapterCapabilities {
            languages: vec!["typescript".into()],
            component_kinds: vec!["service".into()],
            targets: vec!["backend".into()],
            operations: vec!["validate".into(), "compile".into()],
            requires: vec!["node".into()],
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
            adapter: "typescript-backend".into(),
            source: "./service.ts".into(),
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
fn full_round_trip_type_checks_compiles_and_executes_the_real_script() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_greeter(
        &root,
        VALID_SERVICE_TS,
        "hello from the white-box test",
        true,
    );
    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {\"greeting\": \"white-box greeting\"}}",
    )
    .unwrap();

    let ctx = make_ctx(&root);
    let adapter = typescript_backend::build();

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
    assert!(result
        .artifacts
        .iter()
        .any(|a| a.relative_path == "app/service.js"));

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

/// The crux of this adapter: a real type error must fail the build with a
/// genuine `tsc` diagnostic, not a transpile-and-ignore. If this test
/// passed, the adapter would just be stripping types, not checking them.
#[test]
fn a_genuine_type_error_fails_both_validate_and_compile() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_greeter(&root, TYPE_BROKEN_SERVICE_TS, "unused", false);
    let ctx = make_ctx(&root);
    let adapter = typescript_backend::build();

    let validate_diags = adapter.validate(&ctx);
    assert!(validate_diags.has_errors());
    assert!(validate_diags
        .items()
        .iter()
        .any(|d| d.code == "E-TSADAPTER-TYPECHECK-FAILED"));

    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-TSADAPTER-TYPECHECK-FAILED"));
    assert!(result.artifacts.is_empty());
    assert!(!root.join("dist/backend/output.txt").exists());
    assert!(!root.join("dist/backend/app").exists());
}

#[test]
fn non_typescript_source_is_rejected() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let comp_dir = root.join("lib/components/svc.greeter");
    fs::create_dir_all(&comp_dir).unwrap();
    fs::create_dir_all(root.join("backend")).unwrap();
    fs::write(
        comp_dir.join("component.yaml"),
        "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs: {}\nimplementations:\n  backend:\n    adapter: typescript-backend\n    source: ./service.js\n",
    )
    .unwrap();
    fs::write(comp_dir.join("service.js"), "console.log('plain js');\n").unwrap();
    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {}}",
    )
    .unwrap();

    let output = OutputDir::resolve(&root, "dist/backend").unwrap();
    let component = ResolvedComponent {
        name: "svc.greeter".into(),
        version: "1.0.0".into(),
        kind: ComponentKind::Service,
        implementation: ComponentImplementation {
            adapter: "typescript-backend".into(),
            source: "./service.js".into(),
        },
        manifest_dir: root.join("lib/components/svc.greeter"),
        adapter: adapter_manifest(),
    };
    let ctx = BuildContext {
        project_root: root.clone(),
        target: "backend".into(),
        entry_path: Some(root.join("backend/service.json")),
        output,
        components: vec![component],
    };

    let adapter = typescript_backend::build();
    let diags = adapter.validate(&ctx);
    assert!(diags.has_errors());
    assert!(diags
        .items()
        .iter()
        .any(|d| d.code == "E-TSADAPTER-NOT-TYPESCRIPT-SOURCE"));
}

#[test]
fn missing_toolchain_is_reported_without_panicking() {
    let dir = tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    scaffold_greeter(&root, VALID_SERVICE_TS, "unused", true);
    let ctx = make_ctx(&root);

    let adapter = typescript_backend::build_with_candidates(vec![
        "definitely-not-a-real-node-xyz123".to_string(),
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
    fs::write(
        comp_dir.join("component.yaml"),
        "schema: omniforge.dev/component/v1\nname: svc.greeter\nversion: 1.0.0\nkind: service\ninterface:\n  inputs:\n    greeting:\n      type: string\n      required: true\nimplementations:\n  backend:\n    adapter: typescript-backend\n    source: ./service.ts\n",
    )
    .unwrap();
    fs::write(comp_dir.join("service.ts"), VALID_SERVICE_TS).unwrap();
    fs::write(
        root.join("backend/service.json"),
        "{\"schema\": \"omniforge.dev/backend-app/v1\", \"service\": \"svc.greeter\", \"inputs\": {}}",
    )
    .unwrap();
    let ctx = make_ctx(&root);

    let adapter = typescript_backend::build();
    let result = adapter.compile(&ctx);
    assert!(result.diagnostics.has_errors());
    assert!(result
        .diagnostics
        .items()
        .iter()
        .any(|d| d.code == "E-TSADAPTER-MISSING-INPUT"));
    assert!(result.artifacts.is_empty());
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

fn real_adapter_invocation(workdir: &Path) -> (String, std::path::PathBuf) {
    let launcher =
        resolve_executable(&node_candidates()).expect("a usable Node.js must exist for this test");
    let script_path = workdir.join(typescript_backend::script_filename());
    fs::write(&script_path, typescript_backend::script_source()).unwrap();
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
        .any(|d| d.code == "E-TSADAPTER-UNSUPPORTED-OPERATION"));
}

#[test]
fn unsafe_component_source_path_is_rejected() {
    let project_dir = tempdir().unwrap();
    let project_root = fs::canonicalize(project_dir.path()).unwrap();
    let (launcher, script_path) = real_adapter_invocation(&project_root);
    let outside_dir = tempdir().unwrap();
    let outside_root = fs::canonicalize(outside_dir.path()).unwrap();
    let evil_script = outside_root.join("evil.ts");
    fs::write(&evil_script, "console.log('should never run');\n").unwrap();

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
        .any(|d| d.code == "E-TSADAPTER-UNSAFE-PATH"));
}

#[test]
fn adapter_manifest_is_conformant() {
    let adapter = typescript_backend::build();
    let violations = omni_adapter_api::conformance::check_manifest_invariants(&adapter);
    assert!(violations.is_empty(), "{violations:?}");
}
