//! End-to-end tests for the `hello-fullstack` example: build the real
//! `backend` target, start the packaged Python script as a real HTTP
//! server (via `support::spawn_backend_server`), and make real HTTP
//! requests against it (via `support::http_request`), validating responses
//! against the declared `backend/api-contract.json` using
//! `omni_core::spec::ApiResponse::validate_body`.
//!
//! Covers exactly six scenarios: a successful request, invalid request
//! data, an unknown route, a backend startup failure, a timeout (both at
//! server-startup and at the HTTP layer), and a deliberate API contract
//! violation. See `examples/hello-fullstack/README.md` for what's real
//! here and what this test suite does and doesn't cover (it does not drive
//! a real browser; `public/api-client.js` is verified manually).

mod support;

use omni_cli::{orchestrator, registry};
use omni_core::spec::build_report::{TargetReport, TargetStatus};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-fullstack")
}

fn fresh_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    support::copy_dir_all(&example_root(), dir.path()).expect("copy example project");
    dir
}

fn build_backend(root: &Path) -> TargetReport {
    // Real CLI commands always resolve the project root through
    // `loader::find_project_root` (which canonicalizes) before calling
    // `load_project` -- do the same here. Skipping this previously left
    // `loaded.root` non-canonical on Windows (no `\\?\` prefix) while
    // `OutputDir::resolve` always returns a canonical path, so the
    // out-of-process adapter received a `project_root`/`output_dir` pair
    // with mismatched prefixes and rejected it as "outside the project
    // root" -- a test-harness gap, not a real orchestrator bug, but one
    // that was silently masked whenever a stale build cache produced a
    // hit (skipping the adapter call, and the check, entirely).
    let root = omni_core::loader::find_project_root(root).expect("canonicalize project root");
    let root = root.as_path();
    let (loaded, _diags) = omni_core::loader::load_project(root).expect("project loads");
    let (components, comp_diags) =
        omni_core::components::discover_components(root, &loaded.file.components.directory)
            .expect("components discover");
    assert!(!comp_diags.has_errors(), "{:?}", comp_diags.items());
    let adapters = registry::built_in_adapters();
    orchestrator::build_target(
        &loaded,
        "backend",
        &components,
        &adapters,
        orchestrator::Mode::Compile,
    )
}

fn python_exe() -> String {
    let candidates = omni_adapter_api::process::known_aliases("python3");
    omni_adapter_api::process::resolve_executable(&candidates)
        .expect("a python3 toolchain is required to run these tests")
}

fn load_contract(root: &Path) -> omni_core::spec::ApiContract {
    let raw = std::fs::read_to_string(root.join("backend/api-contract.json"))
        .expect("api-contract.json is readable");
    omni_core::spec::api_contract::parse(&raw, "backend/api-contract.json")
        .expect("api-contract.json parses and validates")
}

/// Thin wrapper around the reusable `omni_core::spec::ApiResponse::validate_body`
/// (not a local reimplementation -- see `docs/decisions/0008-api-contract-is-a-custom-format.md`
/// for why this validation now lives in `omni-core` rather than only here).
fn assert_body_matches(response: &omni_core::spec::api_contract::ApiResponse, body: &Value) {
    if let Err(messages) = response.validate_body(body) {
        panic!("response body {body} did not match declared contract schema: {messages:?}");
    }
}

fn body_violates_schema(
    response: &omni_core::spec::api_contract::ApiResponse,
    body: &Value,
) -> bool {
    response.validate_body(body).is_err()
}

/// Shared setup for the four scenarios that exercise the real
/// `svc.api/service.py` script: build it, start it on a fresh ephemeral
/// port, return everything needed to talk to it. The returned `ServerGuard`
/// must be kept alive for the duration of the test (its `Drop` tears the
/// server down).
fn build_and_start_real_backend(
    root: &Path,
) -> (support::ServerGuard, u16, omni_core::spec::ApiContract) {
    let report = build_backend(root);
    assert_eq!(
        report.status,
        TargetStatus::Success,
        "backend build failed: {:?}",
        report.diagnostics
    );

    let contract = load_contract(root);
    let dist_backend = root.join("dist/backend");
    let script = dist_backend.join("app/service.py");
    let config = dist_backend.join("run-config.json");
    assert!(script.is_file(), "built script artifact is missing");
    assert!(config.is_file(), "run-config.json artifact is missing");

    let port = support::pick_free_port();
    let python = python_exe();
    let server = support::spawn_backend_server(
        &python,
        &script,
        &config,
        &dist_backend,
        port,
        Duration::from_secs(10),
    )
    .expect("real backend server starts");

    (server, port, contract)
}

#[test]
fn successful_backend_request_matches_declared_contract() {
    let dir = fresh_copy();
    let (_server, port, contract) = build_and_start_real_backend(dir.path());

    let response = support::http_request(
        "127.0.0.1",
        port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_secs(5),
    )
    .expect("http request succeeds");
    assert_eq!(response.status, 200);

    let body: Value = serde_json::from_str(&response.body).expect("valid JSON body");
    let op = contract
        .operation("/api/greeting", "GET")
        .expect("contract declares GET /api/greeting");
    let response_spec = op
        .responses
        .get("200")
        .expect("contract declares a 200 response");
    assert!(
        response_spec.body.is_some(),
        "200 response declares a body schema"
    );
    assert_body_matches(response_spec, &body);
}

#[test]
fn invalid_request_data_returns_the_declared_400_shape() {
    let dir = fresh_copy();
    let (_server, port, contract) = build_and_start_real_backend(dir.path());

    // Missing the required "message" field entirely.
    let response = support::http_request(
        "127.0.0.1",
        port,
        "POST",
        "/api/echo",
        Some("{}"),
        Duration::from_secs(5),
    )
    .expect("http request succeeds");
    assert_eq!(response.status, 400);

    let body: Value = serde_json::from_str(&response.body).expect("valid JSON body");
    let op = contract
        .operation("/api/echo", "POST")
        .expect("contract declares POST /api/echo");
    let response_spec = op
        .responses
        .get("400")
        .expect("contract declares a 400 response");
    assert!(
        response_spec.body.is_some(),
        "400 response declares a body schema"
    );
    assert_body_matches(response_spec, &body);
}

#[test]
fn unknown_route_returns_a_well_formed_404() {
    let dir = fresh_copy();
    let (_server, port, contract) = build_and_start_real_backend(dir.path());

    let response = support::http_request(
        "127.0.0.1",
        port,
        "GET",
        "/api/does-not-exist",
        None,
        Duration::from_secs(5),
    )
    .expect("http request succeeds");
    assert_eq!(response.status, 404);

    let body: Value = serde_json::from_str(&response.body).expect("valid JSON body");
    assert_eq!(body["error"], "not found");

    // An undeclared route is, by definition, not a path the contract
    // declares an operation for -- see docs/api-contract-format.md's scope
    // note. We assert the server's own well-formed 404 shape directly
    // rather than against the (nonexistent) contract entry for it.
    assert!(contract.operation("/api/does-not-exist", "GET").is_none());
}

#[test]
fn a_response_violating_the_contract_is_detected() {
    let dir = fresh_copy();
    let contract = load_contract(dir.path());

    const WRONG_SHAPE_FIXTURE: &str = r#"
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

args = sys.argv[1:]
port = int(args[args.index("--serve") + 1])


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        pass

    def do_GET(self):
        # Deliberately wrong: "greeting" has the wrong type and "count" is
        # missing entirely, violating the real declared 200 response schema.
        body = json.dumps({"greeting": 12345}).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
print(f"LISTENING {port}", flush=True)
server.serve_forever()
"#;

    let fixture = dir.path().join("wrong_shape.py");
    std::fs::write(&fixture, WRONG_SHAPE_FIXTURE).expect("write fixture");
    let config = dir.path().join("run-config.json");
    std::fs::write(&config, "{}").expect("write placeholder config");

    let port = support::pick_free_port();
    let python = python_exe();
    let _server = support::spawn_backend_server(
        &python,
        &fixture,
        &config,
        dir.path(),
        port,
        Duration::from_secs(10),
    )
    .expect("fixture server starts");

    let response = support::http_request(
        "127.0.0.1",
        port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_secs(5),
    )
    .expect("http request succeeds");
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_str(&response.body).expect("valid JSON body");

    let op = contract.operation("/api/greeting", "GET").unwrap();
    let response_spec = op.responses.get("200").unwrap();
    assert!(
        body_violates_schema(response_spec, &body),
        "expected the deliberately wrong-shaped body {body} to violate the contract, but it matched"
    );
}

#[test]
fn backend_startup_failure_is_reported_without_hanging() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("fails_immediately.py");
    std::fs::write(&fixture, "import sys\nsys.exit(3)\n").expect("write fixture");
    let config = dir.path().join("run-config.json");
    std::fs::write(&config, "{}").expect("write placeholder config");

    let port = support::pick_free_port();
    let python = python_exe();
    // This fixture exits immediately (no sleep, no I/O wait); detection is
    // driven by the child's exit/EOF events, not a polling timeout, so it
    // is correctness-tested here, not timed -- under heavy parallel test
    // load (many Python processes starting at once) a tight wall-clock
    // bound would be flaky for reasons that have nothing to do with the
    // behavior this test cares about. `backend_that_never_becomes_ready_
    // times_out_without_hanging` is the test that exercises the actual
    // timeout-bound code path.
    let err = support::spawn_backend_server(
        &python,
        &fixture,
        &config,
        dir.path(),
        port,
        Duration::from_secs(30),
    )
    .expect_err("a script that exits immediately must not be reported as ready");
    assert!(err.contains("exited"), "unexpected error message: {err}");
}

#[test]
fn backend_that_never_becomes_ready_times_out_without_hanging() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("never_ready.py");
    std::fs::write(&fixture, "import time\ntime.sleep(60)\n").expect("write fixture");
    let config = dir.path().join("run-config.json");
    std::fs::write(&config, "{}").expect("write placeholder config");

    let port = support::pick_free_port();
    let python = python_exe();
    let start = Instant::now();
    let err = support::spawn_backend_server(
        &python,
        &fixture,
        &config,
        dir.path(),
        port,
        Duration::from_millis(300),
    )
    .expect_err("a script that never prints a readiness line must time out");
    assert!(err.contains("readiness"), "unexpected error message: {err}");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "should not wait anywhere near the fixture's full 60s sleep"
    );
}

/// `targets.backend.contract` (set on `hello-fullstack`'s `omni.yaml`)
/// gives the build a real dependency edge on `backend/api-contract.json`:
/// a structurally invalid contract must fail the *build*, not only be
/// caught later by this test file's own HTTP assertions.
#[test]
fn an_invalid_contract_fails_the_backend_build() {
    let dir = fresh_copy();
    std::fs::write(
        dir.path().join("backend/api-contract.json"),
        "{ not valid json",
    )
    .expect("overwrite contract with invalid JSON");

    let report = build_backend(dir.path());
    assert_eq!(
        report.status,
        TargetStatus::Failed,
        "an invalid contract must fail the build, got: {:?}",
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("E-API-CONTRACT")),
        "expected an E-API-CONTRACT* diagnostic, got: {:?}",
        report.diagnostics
    );
}

/// The backend's build cache key must include the contract's bytes: editing
/// only the contract (no component/entry change) must bust the cache
/// rather than silently reusing output built against the old contract.
#[test]
fn editing_the_contract_busts_the_backend_build_cache() {
    let dir = fresh_copy();

    let first = build_backend(dir.path());
    assert_eq!(
        first.status,
        TargetStatus::Success,
        "{:?}",
        first.diagnostics
    );
    assert_eq!(
        first.cache,
        Some(omni_core::spec::build_report::CacheResult::Miss)
    );

    let second = build_backend(dir.path());
    assert_eq!(
        second.status,
        TargetStatus::Success,
        "{:?}",
        second.diagnostics
    );
    assert_eq!(
        second.cache,
        Some(omni_core::spec::build_report::CacheResult::Hit),
        "second build with no changes at all should hit the cache"
    );

    let contract_path = dir.path().join("backend/api-contract.json");
    let mut contract: Value =
        serde_json::from_str(&std::fs::read_to_string(&contract_path).expect("read contract"))
            .expect("parse contract as JSON");
    contract["info"]["version"] = Value::String("9.9.9".to_string());
    std::fs::write(
        &contract_path,
        serde_json::to_string_pretty(&contract).unwrap(),
    )
    .expect("write edited contract");

    let third = build_backend(dir.path());
    assert_eq!(
        third.status,
        TargetStatus::Success,
        "{:?}",
        third.diagnostics
    );
    assert_eq!(
        third.cache,
        Some(omni_core::spec::build_report::CacheResult::Miss),
        "editing only the contract must invalidate the cache, not silently hit"
    );
}

#[test]
fn backend_that_accepts_but_never_responds_times_out_the_http_client() {
    const ACCEPT_BUT_SILENT_FIXTURE: &str = r#"
import socket, sys, time

args = sys.argv[1:]
port = int(args[args.index("--serve") + 1])

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", port))
s.listen(1)
print(f"LISTENING {port}", flush=True)
conn, _ = s.accept()
time.sleep(60)
"#;

    let dir = tempfile::tempdir().expect("tempdir");
    let fixture = dir.path().join("accepts_but_silent.py");
    std::fs::write(&fixture, ACCEPT_BUT_SILENT_FIXTURE).expect("write fixture");
    let config = dir.path().join("run-config.json");
    std::fs::write(&config, "{}").expect("write placeholder config");

    let port = support::pick_free_port();
    let python = python_exe();
    let _server = support::spawn_backend_server(
        &python,
        &fixture,
        &config,
        dir.path(),
        port,
        Duration::from_secs(10),
    )
    .expect("fixture reports ready even though it will never answer a real request");

    let start = Instant::now();
    let result = support::http_request(
        "127.0.0.1",
        port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_millis(500),
    );
    assert!(
        result.is_err(),
        "a server that accepts the connection but never responds must not produce a successful response"
    );
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "should not wait anywhere near the fixture's full 60s sleep"
    );
}
