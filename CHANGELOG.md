# Changelog

## Unreleased — A real Android adapter

- **New target: `android`, via a new `android-gradle` adapter.**
  `omni build android` genuinely generates a minimal Gradle project (one
  `Activity`, one `TextView`) and invokes a real, locally-discovered
  Gradle to produce a real, debug-signed APK — verified by re-opening it
  as a zip archive and confirming `AndroidManifest.xml`/`classes.dex` are
  genuinely present, not merely checking the file exists. See
  [ADR 0011](docs/decisions/0011-android-gradle-adapter.md) for the full
  design, and `examples/hello-android` for a complete, buildable example.
  Verified end to end on this project's own Windows development machine,
  against a real Android Studio SDK/JDK + cached Gradle install, both as
  a crate-level test
  (`crates/omni-android-adapter/tests/compile.rs::builds_a_real_apk_end_to_end`)
  and via the real compiled CLI building `examples/hello-android` and
  inspecting the resulting APK with `aapt2 dump badging`.
- **New architecture note: a third adapter style.** `android-gradle` is
  in-process Rust (registered `in-process/v1`, like `web-static`) that
  safely shells out to a real external build tool (Gradle) which does not
  speak OmniForge's own wire protocol — distinct from `web-static`
  (no subprocess at all) and `python-backend` (`out-of-process/v1`, a
  subprocess that *does* speak the wire protocol). New generic primitives
  in `omni-adapter-api` support this: `process::run_tool` (no-shell,
  controlled-environment, bounded-timeout invocation of an arbitrary
  external tool, capturing stdout/stderr/exit status) and
  `process::controlled_env_with` (the existing fixed environment allowlist
  plus a small, explicit set of extra variables a tool genuinely needs —
  `JAVA_HOME`/`ANDROID_HOME` for Gradle).
- New component template model for Android:
  `crates/omni-android-adapter/src/kotlin.rs` mirrors `web-static`'s
  `template.rs` exactly (fixed `{{field}}` substitution, every placeholder
  a declared input) with a Kotlin-string-literal escaper instead of an
  HTML escaper — notably escaping `$` (Kotlin's string-interpolation
  trigger character), which HTML escaping has no equivalent for.
- New schema `specs/android-app.schema.json`
  (`omniforge.dev/android-app/v1`) for the `android` target's entry file
  (`package`, `app_label`, optional `min_sdk`/`target_sdk`/`compile_sdk`,
  `root`), wired into `omni-core`'s central schema validation
  (`SchemaKind::AndroidApp`) the same way `webapp.schema.json` is.
- New toolchain discovery (`omni_android_adapter::toolchain`): finds a
  real Android SDK, JDK, and Gradle distribution via environment
  variables and common installation locations (including scanning
  Gradle's own wrapper distribution cache,
  `~/.gradle/wrapper/dists/*/*/gradle-*`, so this adapter never needs to
  download Gradle itself) — Windows paths verified directly; macOS/Linux
  default paths implemented but not verified on an actual macOS/Linux
  machine (see ADR 0011). `omni doctor` reports exactly what was found
  (or what's missing) for `android-gradle`, via a dedicated code path
  rather than the generic `capabilities.requires` loop other adapters use.
- **Two real, non-obvious bugs found and fixed while building this**,
  both specific to this adapter, neither a regression in existing code:
  1. A Kotlin/Java JVM-target mismatch (Kotlin defaulting to whatever JDK
     actually runs Gradle, Java defaulting to 1.8) that AGP refuses —
     fixed by pinning both to JVM 17 explicitly in the generated
     `app/build.gradle.kts`.
  2. Gradle's own Java-side path canonicalization cannot parse a
     `\\?\`-prefixed Windows extended-length path (which
     `omni_core::loader::find_project_root` correctly returns) when
     passed as a `--project-dir` *argument* — fixed by never passing that
     argument; the generated project directory is set as the child
     process's working directory instead, which Windows itself handles
     natively regardless of the `\\?\` prefix.
  3. A template file's conventional trailing newline, if not trimmed,
     would splice a raw newline into the generated single-line Kotlin
     string literal, producing invalid Kotlin — fixed by trimming it
     before rendering, with a regression test using a template that
     deliberately has one.
- The real end-to-end Gradle-build test is gated behind an explicit
  `OMNIFORGE_RUN_ANDROID_E2E=1` opt-in (in addition to toolchain
  detection) — unlike Python (ubiquitous on CI/dev machines, confirmed by
  this project's existing tests), a *working* Android SDK/JDK/Gradle/AGP/
  Kotlin combination on an arbitrary machine was not something this
  milestone could verify on GitHub-hosted CI runners; this avoids a real
  CI-image version mismatch silently breaking the shared
  `cargo test --workspace` gate for every contributor. `scripts/verify.sh`
  sets the opt-in and still skips gracefully if the toolchain genuinely
  isn't present. `.github/workflows/ci.yml` gained a separate, isolated
  `android` job (same isolation principle as the existing `browser-e2e`
  job) that always opts in — whether it actually passes on GitHub's
  runners was **not** verified as part of this milestone.
- Added 18 tests (138 total; was 120 after the component-composition
  milestone below): 5 in `omni-adapter-api` (the new
  `run_tool`/`controlled_env_with` primitives), 2 in `omni-core` (the new
  android-app schema), 8 unit + 3 integration in `omni-android-adapter`
  (including the real end-to-end Gradle build, gated as described above).

## Unreleased — Component composition

- **New: component composition.** A component's `component.yaml` may
  declare `composes`: named slots whose value is another, separately
  authored component's fully-rendered, HTML-escaped output — e.g. a
  `ui.page` component embedding a reusable `ui.navbar` component, rather
  than copy-pasting its markup. See
  [ADR 0010](docs/decisions/0010-component-composition-via-slots.md) for
  the full design and why this doesn't reopen
  [ADR 0003](docs/decisions/0003-template-only-rendering.md)'s rejection
  of a general templating language (no loops, no conditionals, no new
  "execute this string" mechanism — just one more recursive call to the
  same fixed renderer).
- `specs/component.schema.json` gained an optional `composes` field
  (object, slot name → `{component, inputs}`), backward compatible
  (every existing component manifest has none and is unaffected).
- `crates/omni-core/src/components.rs::discover_components` validates
  every `composes` entry at discovery time: the named component must
  already be a declared `dependencies` entry
  (`E-COMPONENT-COMPOSE-UNDECLARED-DEPENDENCY`, reusing the resolver's
  existing dependency graph and cycle detection instead of a second one),
  and a slot name must not collide with a declared input name
  (`E-COMPONENT-COMPOSE-NAME-CONFLICT`).
- `crates/omni-web-adapter`: `template.rs`'s renderer gained a two-variant
  `SlotValue` (`Escaped`/`Raw`) so a composed slot's pre-rendered HTML is
  inserted verbatim while every plain input is still always HTML-escaped,
  exactly as before. `lib.rs` gained `render_component`, a recursive
  rendering function (the former inline root-rendering logic, generalized)
  with its own defense-in-depth recursion-stack cycle guard
  (`E-WEB-COMPOSE-CYCLE`) on top of the resolver's graph-level guarantee.
- New example: `examples/navbar-demo` now demonstrates real composition
  (`ui.page` composing `ui.navbar`), with its own README, wired into
  `scripts/verify.sh` and `.github/workflows/ci.yml`.
- Added 5 tests (120 total; was 115): 3 in `omni-core` (undeclared
  dependency rejected, name conflict rejected, valid slot accepted), 1 in
  `omni-web-adapter`'s template renderer (a raw slot value is not
  double-escaped), 1 end-to-end in
  `crates/omni-web-adapter/tests/compile.rs` (a composed child's
  deliberately script-looking input still comes out escaped once spliced
  into the parent's output).
- **Fixed a real, reproducible test-isolation bug**, found while
  re-verifying this milestone (not caused by it):
  `crates/omni-cli/tests/support/mod.rs::copy_dir_all` — used by
  `fullstack_http.rs`/`dev_lifecycle.rs` to give each test its own private
  copy of an example project — copied `dist/` and `.omniforge/` (build
  output and the local build cache) verbatim along with everything else.
  If the real `examples/hello-fullstack` on disk had ever actually been
  built by hand (e.g. running `scripts/verify.sh`, which does exactly
  that), every subsequent test run inherited real, stale cache entries,
  deterministically turning
  `fullstack_http.rs::editing_the_contract_busts_the_backend_build_cache`'s
  expected first-build cache *miss* into a *hit* and failing the test —
  reproduced, root-caused, and fixed in this milestone (now explicitly
  skips copying both directories; verified by re-populating the real
  example's `.omniforge/cache` by hand and re-running the previously
  failing test, which now passes).

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
