# Testing

```sh
cargo test --workspace
```

165 tests as of this writing, all passing, 0 ignored (see `docs/roadmap.md`'s
Testing section for the complete, continuously-updated breakdown — the
table below covers the suites present since this project's earlier
milestones; newer suites are listed in `docs/roadmap.md` and this
project's `CHANGELOG.md` rather than duplicated here verbatim):

| Suite | Count | What it covers |
|---|---|---|
| `omni-core` unit tests | 50 | path safety (`paths.rs`, including a real adversarial symlink-escape test), cache key/store/restore (`cache.rs`), dependency graph ordering + cycle detection (`graph.rs`), project loading + semantic validation (`loader.rs`), JSON Schema validation incl. the adapter manifest schema (`schema.rs`), resolver success/failure paths (`resolver.rs`), diagnostics JSON round-tripping (`diagnostics.rs`), API contract parsing + validation (`spec/api_contract.rs`: valid doc, unknown schema version, missing responses, unknown HTTP method) |
| `omni-core` fixture tests | 4 | `tests/fixtures.rs` against the static files in `fixtures/valid-projects` and `fixtures/invalid-projects` |
| `omni-adapter-api` out-of-process mechanism tests | 23 | `process.rs`: malformed JSON response, non-zero exit with no response, hanging-process timeout, wrong protocol version, mismatched request id, a well-formed response accepted, a missing executable handled cleanly, the controlled-environment allowlist, the materialized-script path-separator guard, `run_tool`/`controlled_env_with` (used by `android-gradle`), and the Windows verbatim-path (`\\?\`) stripping helper. All against real resolved interpreters where relevant — none of this is mocked. |
| `omni-web-adapter` unit tests | 5 | template substitution, HTML escaping, undeclared-placeholder rejection, Unicode passthrough (`template.rs`) |
| `omni-web-adapter` integration tests | 7 | `tests/compile.rs`: a full synthetic project built end-to-end (escaped content, verified assets), a missing-required-input failure that produces zero partial output, a missing-public-asset failure, composed-component rendering, adapter conformance, and two real SEO tests (full `seo:` config producing real meta tags/robots.txt/sitemap.xml; no `site_url` honestly skipping sitemap generation) |
| `python-backend`/`node-backend`/`typescript-backend` adapter white-box tests | 24 (8 each) | `crates/omni-cli/tests/{python,node,typescript}_backend_adapter.rs`: full round trip (real script/compiler executed, artifacts verified), missing toolchain simulated deterministically, missing required input, unsupported operation sent to the real adapter script, an unsafe component source path rejected, a non-zero exit/genuine type error, an inner timeout, and adapter conformance. `typescript-backend`'s round-trip additionally proves genuine `tsc` type-checking via a real `TS2322` error. |
| `android-gradle` adapter tests | 11 | real Gradle invocation gated behind `OMNIFORGE_RUN_ANDROID_E2E=1` — see ADR 0011 |
| `omni-cli` integration tests | 16+ | `tests/cli.rs`: `--version`/`--help`, full `create → check → build → clean` round trip including a cache-hit rebuild, invalid project name rejection, refusing to overwrite a non-empty directory, invalid schema version, missing project file, a build failure from a missing required input, an unknown target name, `adapter list`/`doctor` without a project, duplicate component name detection, a full `backend`-target round trip through the real compiled binary, `package`'s real zip round-trip, and a zip-entry path-safety regression test |
| `omni-cli` build-all partial-failure test | 1 | `tests/build_all_partial_failure.rs`: a project with `web` working and `backend` enabled-but-unimplemented; asserts `omni build all` exits non-zero, `web`'s artifacts survive `backend`'s failure, and `build-report.json` reports each target's status independently |
| `omni-cli` parallel-build tests | 2 | `tests/parallel_build.rs`: two real, different adapters (`web-static` + `node-backend`) built concurrently with verified correct output, plus failure isolation under concurrency |
| `omni-cli` SEO CLI tests | 5 | `tests/seo_check.rs`: `omni seo check`/`omni seo report` against real built output — a passing page, a failing page (missing description/h1/canonical), `report`'s always-zero exit code, a clear error when the target was never built, and valid `--json` output |
| `hello-fullstack` end-to-end HTTP tests | 9 | `tests/fullstack_http.rs`: builds the real `backend` target, starts the packaged Python script as a real HTTP server via a test-only process harness (`tests/support/mod.rs`), and makes real HTTP requests against it — a successful request validated against the declared contract, invalid request data (400), an unknown route (404), a backend startup failure, a backend that never becomes ready (startup timeout), a backend that accepts a connection but never responds (HTTP read timeout), and contract-violation/cache-busting checks. See `examples/hello-fullstack/README.md` for exactly what this does and does not cover (no real browser is driven). |
| `omni dev` lifecycle tests | 8 | `tests/dev_lifecycle.rs`: real process start/stop/port-conflict/crash-detection coverage — see `docs/roadmap.md` |

Run formatting and lints the same way CI does:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Or run everything, plus an end-to-end build of every example
(`hello-web` + `omni seo check`, `hello-backend`, `hello-node`,
`hello-typescript`, `hello-fullstack`, `hello-android` if the toolchain
is present, `navbar-demo`), with:

```sh
bash scripts/verify.sh
```

## What "tested" means here

Every test above asserts on real behavior — actual files on disk, actual
process exit codes, actual subprocesses spawned, actual stderr content —
not mocked internals. The web adapter's integration tests assert on the
*rendered HTML bytes* (e.g. that `<there>` in a user-supplied title
becomes `&lt;there&gt;` in the output, never unescaped). The
`python-backend` tests go one step further: they assert on the *captured
stdout of a real Python process that really ran*
(`dist/backend/output.txt` contains the exact greeting the test
configured, with the exact repeat count requested), and on process-level
failure modes (non-zero exit, a `time.sleep()` that genuinely gets killed
under the enforced timeout) rather than simulating them.

## Testing an out-of-process adapter mechanism without flaky tests

Two choices worth calling out, both explained in more depth in
[decisions/0004-cli-lib-split.md](decisions/0004-cli-lib-split.md):

- **Missing-toolchain is tested with a deterministic bogus candidate
  list** (`python_backend::build_with_candidates(vec!["definitely-not
  -a-real-python-xyz123".into()])`), not by trying to hide the real
  Python installation from the test process via `PATH` manipulation —
  that would behave differently on Windows (which still searches system
  directories regardless of `PATH`) than on Linux/macOS, and would be a
  genuinely flaky test.
- **Protocol edge cases specific to the real adapter script**
  (unsupported operation, unsafe path) are driven by calling
  `omni_adapter_api::process::execute` directly with a hand-crafted
  `AdapterRequest` against the real, embedded `adapter.py` — this needed
  `omni-cli` to expose a `lib` target so its test suite could reach
  `omni_cli::adapters::python_backend::{script_source, script_filename}`.

## Starting a real backend server for testing, without a generic adapter operation

`examples/hello-fullstack`'s service script supports a `--serve <port>`
convention (its own, not an OmniForge mechanism — see
[decisions/0007-backend-dev-server-is-example-scoped.md](decisions/0007-backend-dev-server-is-example-scoped.md)).
`crates/omni-cli/tests/support/mod.rs` provides the process-lifecycle
plumbing `fullstack_http.rs` needs around that: `pick_free_port` (bind an
ephemeral port, drop the listener, hand the number to the child),
`spawn_backend_server` (no shell, controlled environment reused from
`omni_adapter_api::process::controlled_env`, a bounded wait for a readiness
line on stdout), `ServerGuard` (kills and reaps the child in `Drop`, so a
panicking test can't leak an orphan process), and `http_request` (a
hand-rolled, read-timeout-bounded HTTP/1.1 client over `TcpStream` — no new
dependency; this repository's existing style prefers a small hand-rolled
client over pulling in an HTTP crate for a handful of local, synchronous
test requests).

This is test-only code, reviewed with the same process-lifecycle checklist
as `process.rs` (see `docs/security-model.md`), but it is **not** part of
the adapter protocol and is not reachable from `omni build`/`omni dev`.

## Adapter conformance

Every adapter should call
`omni_adapter_api::conformance::check_manifest_invariants` in its own test
suite — both shipped adapters do:
`crates/omni-web-adapter/tests/compile.rs::adapter_manifest_is_conformant`
and `crates/omni-cli/tests/python_backend_adapter.rs::
adapter_manifest_is_conformant`.

## Reproducibility

`crates/omni-cli/tests/cli.rs::create_check_build_clean_round_trip` builds
the same web project twice and asserts the second build reports a cache
hit; the same pattern was verified manually for the `backend` target
(see [caching.md](caching.md)) — the practical form of "the same inputs
produce the same result" available without diffing raw bytes across
OS-dependent timestamp metadata. The build report's artifact list
includes a SHA-256 per file.

## What's not covered yet

- **No automated test drives a real browser.** `examples/hello-fullstack/public/api-client.js`
  is real, shipped browser-side JavaScript (a genuine `fetch()` call), but
  nothing in `cargo test` loads it in a browser engine and checks the page
  renders correctly — that is manually verified (see this example's
  README) and documented in `docs/roadmap.md` as Partial, not claimed as
  automated coverage.
- No test exercises a *third*, non-reviewed/third-party adapter, because
  the trust/enablement policy for one doesn't exist yet (see
  `docs/roadmap.md`). `conformance::check_manifest_invariants` is the
  only thing simulating "a future adapter must satisfy X."
- No fuzz/property testing. `safe_join`'s tests are example-based, not
  property-based, though its logic is small and total (every `Component`
  variant of `std::path::Component` is matched explicitly, no wildcard
  fallthrough). The Python adapter's `is_within()` has the equivalent
  example-based coverage via the "unsafe path" test, not property-based
  either.
- No load/performance testing of the dev server — it is documented as a
  local development convenience, not a production server.
- No test runs `python-backend` against anything other than CPython via
  `python3`/`python`/`py` — e.g. no coverage of PyPy or a virtualenv with
  a renamed interpreter.
