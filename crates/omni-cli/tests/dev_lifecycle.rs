//! End-to-end tests for the unified `omni dev` process-lifecycle runtime
//! (`omni_cli::devrun`): real startup, real HTTP reachability on both the
//! static web target and the real backend dev-server subprocess, real
//! graceful shutdown with real child-process cleanup, and the documented
//! failure modes (port conflicts, a port already in use, a missing
//! toolchain, an unexpected backend crash while running) -- all exercised
//! against the real `hello-fullstack` example, never against a mock.

mod support;

use omni_adapter_api::Adapter;
use omni_cli::devrun::{self, ShutdownSignal};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-fullstack")
}

fn fresh_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    support::copy_dir_all(&example_root(), dir.path()).expect("copy example project");
    dir
}

/// `python-backend` built with a launcher candidate list that cannot
/// possibly resolve to a real executable -- deterministically simulates
/// "Python is not installed" without touching the real process environment
/// (`PATH`), which would be both invasive and flaky for tests that may run
/// in parallel.
fn adapters_with_missing_backend_toolchain() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(omni_web_adapter::WebStaticAdapter::new()),
        Box::new(omni_cli::adapters::python_backend::build_with_candidates(
            vec!["definitely-not-a-real-python-xyz123".to_string()],
        )),
    ]
}

fn immediate_shutdown() -> ShutdownSignal {
    let (tx, rx) = mpsc::channel();
    tx.send(()).expect("send on an unclosed channel");
    ShutdownSignal::Channel(rx)
}

/// `hello-fullstack`'s `omni.yaml` pins fixed dev ports (`web`: 4280,
/// `backend`: 4281) to document the common case -- but tests in this file
/// run concurrently within one process, so reusing those same fixed ports
/// across tests would make them race each other for the same socket. Every
/// test below supplies its own freshly-picked, --port-style overrides
/// instead of relying on the example's configured defaults.
fn fresh_overrides(targets: &[&str]) -> Vec<(String, u16)> {
    targets
        .iter()
        .map(|t| (t.to_string(), support::pick_free_port()))
        .collect()
}

#[test]
fn unified_dev_starts_both_targets_and_shuts_down_cleanly() {
    let dir = fresh_copy();
    let overrides = fresh_overrides(&["web", "backend"]);
    let session = devrun::start(dir.path(), &[], &overrides).expect("dev session starts");
    assert_eq!(
        session.handles.len(),
        2,
        "both web and backend should start"
    );

    let web_port = session
        .handles
        .iter()
        .find(|h| h.target == "web")
        .expect("web target started")
        .port;
    let backend_port = session
        .handles
        .iter()
        .find(|h| h.target == "backend")
        .expect("backend target started")
        .port;

    let web_resp = support::http_request(
        "127.0.0.1",
        web_port,
        "GET",
        "/",
        None,
        Duration::from_secs(5),
    )
    .expect("web dev server responds");
    assert_eq!(web_resp.status, 200);

    let backend_resp = support::http_request(
        "127.0.0.1",
        backend_port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_secs(5),
    )
    .expect("backend dev server responds");
    assert_eq!(backend_resp.status, 200);
    assert!(backend_resp.body.contains("greeting"));

    let start = Instant::now();
    let code = session.wait_and_shutdown(immediate_shutdown());
    assert_eq!(code, 0);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "shutdown must be bounded, not hang"
    );

    // Real cleanup, not assumed: the backend's real child process must no
    // longer be listening.
    let after = support::http_request(
        "127.0.0.1",
        backend_port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_millis(500),
    );
    assert!(
        after.is_err(),
        "backend dev server process should have been stopped, but it still answered"
    );
}

#[test]
fn single_target_dev_still_works_backward_compatibly() {
    let dir = fresh_copy();

    let web_only = devrun::start(dir.path(), &["web".to_string()], &fresh_overrides(&["web"]))
        .expect("single-target web dev session starts");
    assert_eq!(web_only.handles.len(), 1);
    assert_eq!(web_only.handles[0].target, "web");
    web_only.wait_and_shutdown(immediate_shutdown());

    let dir2 = fresh_copy();
    let backend_only = devrun::start(
        dir2.path(),
        &["backend".to_string()],
        &fresh_overrides(&["backend"]),
    )
    .expect("single-target backend dev session starts");
    assert_eq!(backend_only.handles.len(), 1);
    assert_eq!(backend_only.handles[0].target, "backend");
    let port = backend_only.handles[0].port;
    let resp = support::http_request(
        "127.0.0.1",
        port,
        "GET",
        "/api/greeting",
        None,
        Duration::from_secs(5),
    )
    .expect("backend-only dev server responds");
    assert_eq!(resp.status, 200);
    backend_only.wait_and_shutdown(immediate_shutdown());
}

#[test]
fn two_targets_pinned_to_the_same_explicit_port_is_a_conflict_before_anything_starts() {
    let dir = fresh_copy();
    let shared_port = support::pick_free_port();
    let overrides = vec![
        ("web".to_string(), shared_port),
        ("backend".to_string(), shared_port),
    ];
    let err = devrun::start(dir.path(), &[], &overrides)
        .err()
        .expect("a shared explicit port must be rejected");
    assert_eq!(err, 1);
}

#[test]
fn a_port_already_held_by_something_else_is_reported_not_silently_ignored() {
    let dir = fresh_copy();
    let busy_port = support::pick_free_port();
    // Hold the port open for the duration of the attempt.
    let _listener = TcpListener::bind(("127.0.0.1", busy_port)).expect("bind the port first");

    let overrides = vec![("web".to_string(), busy_port)];
    let result = devrun::start(dir.path(), &["web".to_string()], &overrides);
    assert!(
        result.is_err(),
        "starting a dev server on an already-bound port must fail, not silently succeed"
    );
}

#[test]
fn missing_backend_toolchain_still_starts_the_web_target() {
    let dir = fresh_copy();
    let session = devrun::start_with_adapters(
        dir.path(),
        &[],
        &fresh_overrides(&["web"]),
        adapters_with_missing_backend_toolchain(),
    )
    .expect("partial dev session starts (web works even though backend's toolchain is missing)");

    assert_eq!(
        session.handles.len(),
        1,
        "only the web target should have started"
    );
    assert_eq!(session.handles[0].target, "web");
    session.wait_and_shutdown(immediate_shutdown());
}

#[test]
fn missing_toolchain_for_the_only_requested_target_is_a_hard_failure() {
    let dir = fresh_copy();
    let err = devrun::start_with_adapters(
        dir.path(),
        &["backend".to_string()],
        &[],
        adapters_with_missing_backend_toolchain(),
    )
    .err()
    .expect("no targets could start, so this must be an error");
    assert_eq!(err, 1);
}

#[test]
fn an_unexpected_backend_crash_while_running_is_detected_without_hanging_the_other_target() {
    let dir = fresh_copy();
    // Overwrite the backend's component script with a dual-mode fixture
    // matching the real script's own convention (one-shot when `--serve`
    // is absent -- which is what `omni build` actually invokes -- real
    // server mode when present): it reports ready, then exits shortly
    // after, simulating a real crash while `omni dev` is supervising it.
    let crashy = "import sys\nargs = sys.argv[1:]\nif '--serve' not in args:\n    print('ok')\n    sys.exit(0)\nport = int(args[args.index('--serve') + 1])\nprint(f'LISTENING {port}', flush=True)\nsys.exit(1)\n";
    std::fs::write(dir.path().join("lib/components/svc.api/service.py"), crashy)
        .expect("overwrite backend script with a crashy fixture");

    let overrides = fresh_overrides(&["web", "backend"]);
    let session = devrun::start(dir.path(), &[], &overrides).expect("dev session starts");
    assert_eq!(session.handles.len(), 2);
    let web_port = session
        .handles
        .iter()
        .find(|h| h.target == "web")
        .unwrap()
        .port;

    // Give the backend time to crash and the supervisor loop (200ms poll
    // interval) time to notice.
    std::thread::sleep(Duration::from_millis(800));

    // The web target must still be fine -- one target crashing does not
    // take down the whole session.
    let web_resp = support::http_request(
        "127.0.0.1",
        web_port,
        "GET",
        "/",
        None,
        Duration::from_secs(5),
    )
    .expect("web target keeps serving after the backend crashed");
    assert_eq!(web_resp.status, 200);

    let start = Instant::now();
    let code = session.wait_and_shutdown(immediate_shutdown());
    assert_eq!(code, 0);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "shutdown must stay bounded even after a managed process already crashed"
    );
}

/// `ManagedProcess::stop`'s graceful-then-hard-kill guarantee, exercised
/// through the real `omni dev` path rather than only at the unit level in
/// `omni-adapter-api` -- the real backend script
/// (`examples/hello-fullstack`'s `service.py`) is what actually gets
/// stopped here, on whichever platform this test runs on.
#[test]
fn real_backend_dev_server_is_actually_stopped_within_the_configured_grace_period() {
    let dir = fresh_copy();
    let session = devrun::start(
        dir.path(),
        &["backend".to_string()],
        &fresh_overrides(&["backend"]),
    )
    .expect("backend-only dev session starts");
    let port = session.handles[0].port;
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_err(),
        "port should be in use by the real backend process"
    );

    let start = Instant::now();
    session.wait_and_shutdown(immediate_shutdown());
    // adapters/python-backend/adapter.yaml declares stop_grace_ms: 3000;
    // allow headroom for process teardown itself but still prove this
    // does not silently fall back to waiting much longer than that.
    assert!(
        start.elapsed() < Duration::from_secs(8),
        "stop must respect the adapter's configured grace period, not hang indefinitely"
    );
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "the port must be free again after shutdown -- the real child process is gone"
    );
}
