# Changelog

## Unreleased — Real packaging, and a public release

- New `omni package <target|all>`: archives an already-built target's
  `dist/<target>/` into a single `dist/<target>.zip`. Generic across any
  target/adapter (packaging a flat output directory needs no
  adapter-specific knowledge) — implemented once at the CLI level, not as
  a new adapter-protocol operation.
- **Corrected a false capability claim**: `adapters/web-static/adapter.yaml`
  previously declared `package` as a supported operation despite the
  `Adapter` trait never having a `package` method and nothing ever
  dispatching it. Removed the claim; `omni package` above is the real
  functionality in its place.
- Added 4 tests (`crates/omni-cli/tests/cli.rs`): a real zip archive is
  produced and read back (not just checked for existence) with content
  matching the on-disk build output, `package all`, and the two failure
  paths (not yet built, unknown target name).
- Published this repository to GitHub for the first time:
  https://github.com/shariful-islam-raiyan/Omni-Forge — added a
  professional README, CONTRIBUTING/SECURITY/CODE_OF_CONDUCT updates,
  GitHub issue/PR templates, and a root `ROADMAP.md`. Verified the
  documented `cargo install --git` installation workflow end-to-end
  (including `omni create`/`check`/`build`/`dev`) from a build produced
  in a directory with no relationship to the original development tree.
- Added 115 tests total (was 111), all passing.

## Unreleased — Unified Application Runtime and Developer Experience

- **`omni dev` is now a real, unified, multi-target dev runtime.**
  `omni dev [targets...] [--port NAME=PORT]` builds and serves every
  enabled (or named) target at once: a managed, supervised, real child
  process for any target whose adapter declares dev-serve support (today:
  `backend`), the CLI's own static file server for in-process targets
  (now itself stoppable, and with a per-connection read timeout). Ports
  auto-pick free by default; pin one via `omni.yaml`'s `targets.<name>.dev.port`
  or `--port`. Two targets pinned to the same port is a startup-time
  `E-DEV-PORT-CONFLICT`. See
  [docs/dev-workflow.md](docs/dev-workflow.md) and
  [ADR 0009](docs/decisions/0009-unified-dev-process-lifecycle.md).
- New `omni_adapter_api::devserver` (promoted from a test-only harness):
  spawn + readiness-line detection + bounded timeout, and a real
  graceful-then-hard-kill `stop()` — `SIGTERM` then `SIGKILL` on Unix,
  `CTRL_BREAK_EVENT` then `TerminateProcess` on Windows (verified
  empirically on both the unit-test level and end-to-end against the real
  compiled binary with a genuine OS signal).
- New adapter-manifest capability: `adapter.yaml`'s optional `dev` block
  plus `Adapter::dev_serve_plan`, letting any out-of-process adapter opt
  into dev-serve support by following a documented "runnable-package
  convention" — not a new wire-protocol operation. `python-backend` is the
  one adapter that declares it today. See
  [docs/adapter-protocol.md](docs/adapter-protocol.md).
- **Corrected a false compatibility claim**: the API contract format
  (`specs/api-contract.schema.json`) previously described itself as "a
  subset of OpenAPI 3.1." It isn't — no `openapi` field, no
  `content`/media-type wrapping, draft-07 rather than OpenAPI 3.1's
  2020-12-based dialect. Renamed/redocumented as the small custom
  OmniForge format it actually is; no wire-format change. See
  [ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md).
- Added reusable contract validation to the library:
  `ApiOperation::validate_request_body`/`ApiResponse::validate_body`
  (`crates/omni-core/src/spec/api_contract.rs`), replacing ad hoc
  `jsonschema` calls that only lived in one test file.
- **A target's `contract:` is now a real build dependency**, not only a
  test-time check: `omni build`/`omni check` schema-validate the declared
  `api-contract.json` as part of building that target, and its bytes are
  now part of the build cache key (previously, editing the contract did
  not invalidate the cache).
- New real-browser E2E suite (`integration-tests/browser/`, Playwright/
  Chromium, Node-based, additive to — not a replacement for — the Rust
  test suite). Running it for real found and led to fixing a genuine bug:
  the full-stack example's backend never sent CORS headers, so a real
  browser's cross-origin `fetch()` silently failed — something the
  previous "manually verified" claim had not actually caught.
- New `crates/omni-cli/tests/dev_lifecycle.rs` (8 tests) and
  `omni-adapter-api::devserver` unit tests (7 tests) covering the above:
  real startup/shutdown, real child-process cleanup, port conflicts, a
  port already in use, a missing toolchain (partial session and hard
  failure), an unexpected mid-session crash, backward-compatible
  single-target invocation, and bounded graceful-then-hard-kill timing.
- Fixed a test-harness bug (not a production one): `fullstack_http.rs`'s
  `build_backend` helper skipped the canonicalization every real CLI
  command performs via `loader::find_project_root`, which could make the
  out-of-process adapter reject a non-canonical `project_root`/canonical
  `output_dir` pair as "outside the project root" on Windows — previously
  masked whenever a stale build cache produced a hit (skipping the
  adapter call, and the check, entirely).
- Added 23 tests (111 total; was 88), all passing, plus 3 additional
  real-browser tests outside `cargo test`.
- `.github/workflows/ci.yml` gained a separate `browser-e2e` job (so a
  Node-side issue never blocks the core Rust gate).

## Unreleased — Full-stack example and API contracts

- New versioned, language-neutral API contract format
  (`specs/api-contract.schema.json`, `omniforge.dev/api-contract/v1`): a
  small custom OmniForge format describing HTTP methods, paths,
  request/response schemas (inline JSON Schema draft-07 fragments), and
  error responses. Loaded/validated by
  `crates/omni-core/src/spec/api_contract.rs`. See
  [docs/api-contract-format.md](docs/api-contract-format.md) and
  [ADR 0006](docs/decisions/0006-api-contract-format.md). (Originally
  described here as "a subset of OpenAPI 3.1" — corrected in the
  Unified Application Runtime milestone below; see
  [ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md).)
- New `examples/hello-fullstack`: one project, `web` and `backend` both
  enabled. The backend component script (`svc.api/service.py`) gained a
  `--serve <port>` mode (a documented script convention, not a protocol
  change — [ADR 0007](docs/decisions/0007-backend-dev-server-is-example-scoped.md))
  that starts a real stdlib HTTP server implementing `GET /api/greeting`
  and `POST /api/echo`, exactly as declared in
  `examples/hello-fullstack/backend/api-contract.json`. Its default
  invocation (what `omni build backend` actually runs) is byte-for-byte
  the same one-shot contract every other python-backend component already
  uses — nothing about the build path changed.
- New real browser-side API client (`examples/hello-fullstack/public/api-client.js`),
  shipped through the existing, unmodified `web-static` `public.js` asset
  mechanism — no adapter code changed. Real, documented, and manually
  verified; not covered by an automated browser test this milestone (see
  the example's README and `docs/roadmap.md`).
- New end-to-end test suite `crates/omni-cli/tests/fullstack_http.rs` (7
  tests) plus a test-only process-lifecycle harness
  (`crates/omni-cli/tests/support/mod.rs`): builds the real backend,
  starts it as a real subprocess, and makes real HTTP requests against it
  — success, invalid request data, an unknown route, a backend startup
  failure, a startup timeout, an HTTP-level read timeout, and a
  deliberate contract violation. `omni_adapter_api::process::controlled_env`
  made `pub` so this harness reuses the exact same environment allowlist
  as the real adapter mechanism.
- New `crates/omni-cli/tests/build_all_partial_failure.rs`, proving (not
  changing) that `omni build all` already reports each enabled target's
  status independently and that one target's failure doesn't erase
  another's artifacts.
- `scripts/verify.sh` and `.github/workflows/ci.yml` now also build
  `examples/hello-fullstack` with `omni build all` and verify both
  targets' artifacts.
- Added 12 tests (76 → 88), all passing: 4 API contract schema/parse
  tests, 7 full-stack HTTP end-to-end tests, 1 build-all partial-failure
  test.

## Unreleased — Out-of-process adapters

- Real out-of-process adapter execution engine
  (`omni-adapter-api::process`): safe argv-only spawning, controlled
  working directory and environment allowlist, enforced timeouts via
  polling + kill, and full protocol validation (empty/malformed response,
  protocol version/request-id mismatch, unsafe artifact paths).
- New `python-backend` adapter (`adapters/python-backend/adapter.py`): a
  real, stdlib-only, out-of-process Python adapter for `service`-kind
  components on the `backend` target. Validates inputs, actually executes
  the component's script with a real Python interpreter, and produces a
  runnable package (`app/`, `run.sh`, `run.bat`) plus proof-of-execution
  artifacts (`output.txt`, `run-config.json`, `build-manifest.json`).
- New `examples/hello-backend` end-to-end example.
- `crates/omni-cli` gained a `lib` target so adapter failure modes
  (missing toolchain, unsafe paths, unsupported operations) can be tested
  deterministically rather than through flaky environment manipulation.
- Fixed: `omni doctor` could report a working toolchain as missing on
  Windows installs where `python`/`python3` on `PATH` are broken Store
  alias stubs; it now tries the same platform-aware candidate list
  (`python3`, `python`, `py`) adapter invocation itself uses.
- Fixed: `Diagnostic`'s optional fields required `#[serde(default)]` to
  deserialize from JSON that omits an absent field rather than writing it
  as explicit `null` — affects any adapter (not just `python-backend`)
  that hand-writes protocol responses.
- Fixed: `omni clean` could transiently fail on Windows immediately after
  a build that wrote new files, due to real-time antivirus scanning
  locking them briefly; directory removal now retries briefly before
  surfacing a persistent failure.
- Renamed the adapter manifest schema's out-of-process protocol tag from
  the placeholder `jsonrpc/v1` to `out-of-process/v1` (no out-of-process
  adapter existed yet when the placeholder was chosen).
- Added 26 tests (50 → 76), all passing: 11 generic out-of-process
  mechanism tests, 8 `python-backend` white-box tests, 1 CLI-level
  backend end-to-end test, 4 adapter-manifest schema tests, 2 diagnostics
  round-trip tests.

## 0.1.0 — Initial MVP

- Language-neutral project/component/adapter specification (v1 schemas).
- Core engine: config loading + JSON Schema validation, component discovery,
  dependency graph with cycle detection, deterministic build planning,
  content-addressed local build cache.
- In-process `web-static` adapter: renders escaped-template components to a
  real static site (HTML/CSS/JS/public assets) with artifact verification.
- CLI: `create`, `check`, `adapter list`, `doctor`, `build`, `dev`, `test`,
  `clean`.
- Structured build reports and stable diagnostic codes.
