# Roadmap

Status definitions: **Implemented** — has passing tests and works today.
**Partial** — the type/field/hook exists and is exercised, but the
behavior it implies isn't fully realized. **Planned** — designed or
discussed, nothing built. **Blocked** — needs a decision or external
resource (a toolchain, a sandboxing design) this milestone didn't produce.

## Specification layer

| Item | Status | Notes |
|---|---|---|
| Project spec v1 (`omni.yaml`) | Implemented | `specs/project.schema.json` + `loader.rs` |
| Component spec v1 (`component.yaml`) | Implemented | `specs/component.schema.json` + `components.rs` |
| Adapter spec v1 (`adapter.yaml`) | Implemented | `specs/adapter.schema.json`; two real manifests exist (`web-static`, `python-backend`) with distinct `protocol` tags (`in-process/v1`, `out-of-process/v1`) |
| Web app manifest v1 | Implemented | `specs/webapp.schema.json`; consumed only by `web-static` |
| Backend app manifest v1 | Implemented | `omniforge.dev/backend-app/v1`, plain JSON; consumed only by `python-backend`, not core-validated (deliberately — see `docs/component-specification.md`) |
| Build report v1 | Implemented | `specs/build-report.schema.json` |
| API contract spec (`api-contract.json`) | Implemented | `specs/api-contract.schema.json` (`omniforge.dev/api-contract/v1`), a small custom OmniForge format (not OpenAPI — see [ADR 0008](decisions/0008-api-contract-is-a-custom-format.md)); loaded/validated by `crates/omni-core/src/spec/api_contract.rs`, including reusable `validate_request_body`/`validate_body`; validated as part of `omni build`/`omni check` when a target declares `contract:` in `omni.yaml`; worked example in `examples/hello-fullstack/backend/api-contract.json`; see [docs/api-contract-format.md](api-contract-format.md) |
| Real OpenAPI 3.1 document support (`$ref`, `components`, media-type-aware bodies, 2020-12 dialect) | Planned | Not implemented; the contract format above is intentionally a smaller, custom format instead — see [ADR 0008](decisions/0008-api-contract-is-a-custom-format.md) |
| Schema version migration tooling | Planned | Only one version of each schema exists; nothing to migrate from. The adapter schema's `protocol` enum *was* migrated once already (`jsonrpc/v1` → `out-of-process/v1`, before any out-of-process adapter existed to depend on the old name) — see [ADR 0005](decisions/0005-out-of-process-python-backend.md). |

## Core engine

| Item | Status | Notes |
|---|---|---|
| Config load + validation | Implemented | |
| Component discovery + validation | Implemented | |
| Adapter capability checks before dispatch | Implemented | `resolver.rs` |
| Dependency graph + cycle detection | Implemented | `graph.rs`, deterministic ordering |
| Build-plan generation | Implemented | `ResolvedBuildPlan` |
| Local build cache | Implemented | Scoped as described in `caching.md`; works identically for the `backend` target |
| Parallel task execution | Planned | Graph output is schedule-ready; no scheduler exists; everything runs sequentially, including out-of-process adapter calls |
| Diagnostics with stable codes | Implemented | ~60 codes across the codebase, including the `E-ADAPTER-*` (mechanism) and `E-PYADAPTER-*` (python-backend business logic) families |

## Adapters

| Item | Status | Notes |
|---|---|---|
| `web-static` (HTML/CSS/JS/assets, in-process) | Implemented | Full round trip tested |
| Out-of-process adapter execution mechanism | Implemented | `crates/omni-adapter-api/src/process.rs`: safe argv spawning, controlled env/cwd, enforced timeout, full protocol validation. Language-neutral — not Python-specific. |
| `python-backend` (runs a real Python script, out-of-process) | Implemented | `adapters/python-backend/adapter.py`, stdlib-only; see `docs/component-specification.md` for its contract. Proven end to end: `examples/hello-backend`, `crates/omni-cli/tests/python_backend_adapter.rs`, `crates/omni-cli/tests/cli.rs::build_backend_target_executes_real_python_script_end_to_end` |
| Third-party (non-reviewed) adapter trust/enablement policy | Planned | `python-backend` does **not** satisfy this — it's shipped and reviewed in this repository, embedded at compile time, not discovered or downloaded. See `adapter-protocol.md`'s policy section. |
| Flutter/Dart adapter (Android/iOS/Windows/Linux/macOS) | Planned | Needs the Flutter SDK available in the build environment, and a packaging/signing story per platform — neither exists here |
| Dev-serve capability (`adapter.yaml`'s optional `dev` block + `Adapter::dev_serve_plan`) | Implemented | Not a wire-protocol operation (the `Operation` enum's reserved `Run`/`Debug`/`Inspect` variants are still unimplemented) — a separate, manifest-declared capability describing how to start an adapter's *build output* as a long-lived dev server. Generic across any `ProcessAdapter` via the "runnable-package convention" (`docs/adapter-protocol.md`); `python-backend` is the one adapter that declares it today. Supersedes ADR 0007's "manual step" framing — see [ADR 0009](decisions/0009-unified-dev-process-lifecycle.md). |
| Rust backend adapter | Planned | Could reuse the same `ProcessAdapter` mechanism (spawn a compiled binary instead of `python + script`) with no changes to `process.rs` |
| Go backend adapter | Planned | Same mechanism reuse applies |
| C/C++ backend adapter | Planned | |
| COBOL adapter | Planned | Genuinely speculative; no concrete design started |
| WebAssembly Component Model / WIT cross-language contracts | Planned | |

## Full-stack example

| Item | Status | Notes |
|---|---|---|
| `examples/hello-fullstack` (one project, `web` + `backend` both enabled) | Implemented | `omni build all` builds both targets independently from one `omni.yaml`; `omni dev` runs both as real, supervised dev servers; see the example's own README |
| Backend real HTTP endpoint | Implemented | `svc.api/service.py --serve <port>` (a documented script convention, not a protocol feature — [ADR 0007](decisions/0007-backend-dev-server-is-example-scoped.md)); now actually started by `omni dev` itself (via the `dev` manifest capability — [ADR 0009](decisions/0009-unified-dev-process-lifecycle.md)), not only by hand or by a test harness; exercised by real subprocess + real HTTP requests in `crates/omni-cli/tests/fullstack_http.rs` and `crates/omni-cli/tests/dev_lifecycle.rs` |
| Browser-side API client (`public/api-client.js`) | Implemented | Real, shipped `fetch()`-based JavaScript, served verbatim by the unmodified `web-static` adapter. Now covered by a real headless-Chromium Playwright suite (`integration-tests/browser/`) driven against a real `omni dev` process — this is how a real, previously-undetected CORS bug (the backend never sent `Access-Control-Allow-Origin`, so a real browser's `fetch` silently failed) was found and fixed; see `integration-tests/browser/README.md` |
| `omni build all` partial-failure reporting | Implemented | Already worked before this milestone (`crates/omni-cli/src/commands/build.rs`); proven by `crates/omni-cli/tests/build_all_partial_failure.rs` |
| `targets.<name>.contract` build-time validation + cache dependency | Implemented | `omni build`/`omni check` schema-validate the declared `api-contract.json` and include its bytes in that target's cache key — previously this validation only ever ran inside a hand-written test, and editing the contract did not invalidate the cache; see `crates/omni-cli/tests/fullstack_http.rs::an_invalid_contract_fails_the_backend_build`/`editing_the_contract_busts_the_backend_build_cache` |

## Component system

| Item | Status | Notes |
|---|---|---|
| `ui` kind, executable | Implemented | via `web-static` |
| `service` kind, executable | Implemented | via `python-backend` |
| `logic`/`library`/`tool` kinds, modeled | Partial | Schema accepts and validates them; no adapter executes them |
| Multiple candidate implementations per target with a selection rule | Blocked | Format currently allows only one implementation per target per component, so there's no selection to make yet; needs a format decision first |
| Component composition (one component's rendered output embedded in another's, via `composes`) | Implemented | `specs/component.schema.json`, `crates/omni-core/src/components.rs` (validation), `crates/omni-web-adapter/src/lib.rs::render_component` (recursive rendering); see [ADR 0010](decisions/0010-component-composition-via-slots.md) and `examples/navbar-demo`. `web-static` only; a composed slot's `inputs` are static (not interpolated from the parent's own inputs) |
| Component outputs/events, modeled | Partial | Schema + types exist (`ComponentInterface::outputs`/`events`); neither shipped adapter reads or emits them |

## CLI

| Item | Status | Notes |
|---|---|---|
| `create` | Implemented | Still web-only scaffolding (`templates/basic`); no `omni create --target backend` yet |
| `check` [`--target`, `--json`] | Implemented | Works for `backend` exactly as for `web` — spawns the adapter process for validation too |
| `adapter list` [`--json`] | Implemented | Lists both adapters, their protocol tag, and toolchain requirements |
| `doctor` | Implemented | Uses the same toolchain-alias resolver as adapter invocation (`known_aliases`), so it reports availability accurately instead of checking only the literal requirement name |
| `build <target\|all>` [`--allow-partial`] | Implemented | |
| `dev [targets...]` [`--port NAME=PORT`] | Implemented | Unified: builds and serves every enabled target (or just the ones named) at once. `web`-like (in-process) targets get the built-in static file server; targets whose adapter declares a `dev` capability (today: `python-backend`) get a real, supervised, long-lived child process with graceful-then-hard-kill shutdown. Ports auto-pick a free one unless pinned via `omni.yaml`'s `targets.<name>.dev.port` or `--port`; two targets pinned to the same port is a startup-time `E-DEV-PORT-CONFLICT`, not a race. See [ADR 0009](decisions/0009-unified-dev-process-lifecycle.md). |
| `test` | Partial | Validates the project and reports, per enabled target, whether its adapter supports the `test` operation (neither does) — correctly reports "no tests were run" rather than faking a pass |
| `package <target\|all>` | Implemented | Archives an already-built target's `dist/<target>/` into `dist/<target>.zip` — generic across any target/adapter (packaging a flat output directory needs no adapter-specific knowledge). Corrects a prior gap: `web-static`'s manifest used to declare `package` as a supported *operation* despite the `Adapter` trait never having a `package` method and nothing ever calling it; that false capability claim has been removed, and this is the real, tested functionality in its place. |
| `clean` | Implemented | Scoped strictly to the resolved output directory; retries briefly on a transient Windows file-lock before failing |
| Machine-readable (`--json`) output everywhere | Partial | Present on `check` and `adapter list`; not yet on `build`/`doctor` |

## Security

| Item | Status | Notes |
|---|---|---|
| Path traversal protection | Implemented | `safe_join` (Rust) + `is_within()` (Python, defense in depth), both tested |
| Safe output/clean handling | Implemented | `OutputDir`, tested, with retry for a real observed Windows AV-lock flake |
| Safe subprocess invocation | Implemented | `crates/omni-adapter-api/src/process.rs`: argv-only, controlled cwd/env, enforced timeout — used by `doctor`'s tool checks, toolchain resolution, and every `python-backend` invocation |
| Secret redaction in reports/cache/logs | Implemented | Adapter child processes get an explicit environment allowlist, not the parent's full environment (`controlled_env`); nothing in the pipeline reads env vars or secret stores otherwise |
| Dependency pinning | Implemented | `Cargo.lock` committed |
| Out-of-process execution mechanism hardening | Implemented | Timeout + kill, protocol/version/id validation, unsafe-artifact-path rejection — all tested in `process.rs` |
| Adapter sandboxing (process isolation beyond argv/env/cwd/timeout control) | Blocked | No *third-party* adapter code exists to sandbox — both shipped adapters are reviewed, repository-embedded code (ADR 0002, ADR 0005); real sandboxing is its own project when that changes |
| Dependency audit tooling (`security.audit_dependencies`) | Partial | Config field exists, validated, not yet wired to a checker |
| Artifact signing | Planned | |

## Testing

| Item | Status | Notes |
|---|---|---|
| Unit tests (config, schema, resolver, graph, cache, paths, diagnostics, api_contract, component composition validation) | Implemented | 47 tests |
| Fixture-driven tests | Implemented | 4 tests against `fixtures/` |
| Out-of-process mechanism tests | Implemented | 11 tests, `omni-adapter-api::process` (malformed response, timeout, nonzero exit, missing executable, protocol/id mismatch, controlled env, path-separator rejection) |
| Dev-server process-lifecycle unit tests | Implemented | 7 tests, `omni-adapter-api::devserver` (readiness detection, ready-timeout, early exit, graceful stop, hard-kill fallback on both platforms this milestone's own CI matrix covers, already-exited no-op, free-port allocation) |
| `web-static` adapter integration tests | Implemented | 5 tests, including a composed-component end-to-end render |
| `python-backend` adapter white-box tests | Implemented | 8 tests, `crates/omni-cli/tests/python_backend_adapter.rs` (full round trip, missing toolchain, missing required input, unsupported operation, unsafe path, inner nonzero exit, inner timeout, conformance) |
| CLI integration tests | Implemented | 16 tests, `omni-cli` (10 web/general + 1 backend end-to-end + 4 `package` command, real zip archive read back + 1 build-all partial-failure) |
| `hello-fullstack` end-to-end HTTP tests | Implemented | 9 tests, `crates/omni-cli/tests/fullstack_http.rs`: success, invalid request data, unknown route, backend startup failure, startup timeout, HTTP read timeout, contract violation, invalid-contract-fails-the-build, contract-edit-busts-the-cache — against a real HTTP server, real requests, no mocking |
| Unified `omni dev` lifecycle tests | Implemented | 8 tests, `crates/omni-cli/tests/dev_lifecycle.rs`: both targets start and respond, graceful shutdown really stops the real child and frees its port, two targets pinned to one port is a startup-time conflict, a port already held by something else is reported, a missing toolchain still starts the other target (and is a hard failure when it's the only one), an unexpected mid-session crash is detected without hanging the other target, single-target invocation stays backward compatible, and the real backend's graceful-then-hard-kill stop is bounded by its configured grace period |
| Real-browser E2E (Playwright/Chromium) | Implemented, outside `cargo test` | 3 tests, `integration-tests/browser/`: page load + real client-side fetch renders a real backend response, a real browser-executed `fetch` round-trips `POST /api/echo`, and the declared 400 shape is returned for an invalid request — all against a real `omni dev` process. Requires Node.js; not part of `cargo test`/`scripts/verify.sh`. Found and led to fixing a real bug (missing CORS headers on the example backend) that no prior "manual verification" claim had actually caught. |
| Adapter conformance suite | Implemented | `check_manifest_invariants`; both shipped adapters pass it |
| Reproducibility check | Partial | Cache-hit-on-rebuild is tested for both targets; no byte-for-byte diff across OS/machine boundaries has been attempted |
| Fuzz/property testing | Planned | |

## SEO / web features

| Item | Status | Notes |
|---|---|---|
| Metadata/canonical URLs/sitemap/Open Graph/structured data/prerendering | Planned | None of this exists; `web-static` emits one page with a `<title>` and nothing else in `<head>` |

## Platform-native features (file access, notifications, permissions, device APIs)

| Item | Status | Notes |
|---|---|---|
| Any of the above | Blocked | No native-target adapter exists yet to define a capability surface for |
