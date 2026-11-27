# Security model

This document states plainly what is and isn't implemented. Overclaiming
here is exactly the failure mode the brief this project was built against
warns against, so: nothing below is aspirational.

## Threat model

OmniForge, as shipped, runs as the `omni` CLI process plus, per build, at
most one short-lived child process per out-of-process adapter it invokes
(today: `python-backend` and `node-backend`, each spawned once per
`validate`/`compile` call,
and — a different style, see below — `android-gradle`'s real Gradle
invocation during `compile()`) — operating on a project directory the
invoking user already controls. `omni build`/`omni check` never leave a
process running afterward. `omni dev` is the one exception: for the
duration it runs, it may additionally supervise a real, long-lived child
process (a `dev`-capable adapter's backend dev server) and/or a static
file server, both bound to `127.0.0.1` only. The things worth defending
against today are:

1. A malformed or malicious `omni.yaml`/`component.yaml`/app-manifest
   trying to make OmniForge read or write outside the project directory.
2. A component template trying to inject unescaped HTML/script content
   into generated output via a crafted input value.
3. `omni clean` being pointed at the wrong directory and deleting
   something it shouldn't.
4. The dev server being asked for a path outside its serve root.
5. An out-of-process adapter's child process hanging, crashing, returning
   garbage, or claiming to have written files it didn't actually write.
6. An adapter child process inheriting more of the parent's environment
   or privileges than it needs.
7. `omni dev`'s long-lived child process outliving the `omni` process
   that started it (a leaked/orphaned server after Ctrl-C, a crash, or a
   panic).
8. `android-gradle`'s real Gradle invocation, which (unlike every other
   adapter here) can perform real network access on a cold cache to
   resolve the Android Gradle Plugin/Kotlin Gradle Plugin, hanging or
   misbehaving.

## What's implemented

- **Path containment** (`omni_core::paths::safe_join`, `OutputDir`). Every
  configurable path (`components.directory`, `build.output`, a target's
  `entry`, a component implementation's `source`, the dev server's
  requested URL path) is resolved through `safe_join`, which rejects
  absolute paths outright and walks `..` components manually, refusing
  any traversal that would leave the trusted base — tested in
  `crates/omni-core/src/paths.rs` (`rejects_absolute_paths`,
  `rejects_parent_escape`, plus a test that *internal* `..` which stays
  inside the base is still allowed, since that's not a security issue).
  `OutputDir::resolve` additionally canonicalizes both the target and the
  project root and verifies containment with the *canonical* paths (so a
  pre-existing symlink can't be used to escape after the fact). The
  Python adapter re-implements the same containment check itself
  (`is_within()` in `adapters/python-backend/adapter.py`, using
  `os.path.realpath` + `os.path.commonpath`) as defense in depth — tested
  directly in `crates/omni-cli/tests/python_backend_adapter.rs::
  unsafe_component_source_path_is_rejected`.
- **Safe cleanup, with a bounded retry for a real Windows flake.** `omni
  clean` only ever calls `OutputDir::remove_all`, which re-verifies
  canonical containment at delete time (not just at resolve time) and
  explicitly refuses to delete the project root itself
  (`output_dir_refuses_to_delete_project_root` test). Deleting the
  directory itself retries briefly on failure
  (`remove_dir_all_with_retry`) because a file written moments earlier
  (e.g. a materialized adapter script) can be transiently locked by
  real-time antivirus scanning on Windows — observed directly while
  building the `python-backend` adapter, not a hypothetical.
- **No code execution in `web-static` rendering.** Component rendering
  (`crates/omni-web-adapter/src/template.rs`) only substitutes declared
  `{{field}}` placeholders and HTML-escapes every value — see
  [decisions/0003-template-only-rendering.md](decisions/0003-template-only-rendering.md).
  There is no `eval`, no shell-out, no embedded interpreter. Component
  **composition** (`composes` in `component.yaml`,
  [ADR 0010](decisions/0010-component-composition-via-slots.md)) is the
  one addition to this since ADR 0003: a component may embed another
  component's output as a placeholder's value. This does not add code
  execution or a new unescaped-input path — a composed slot's value is
  always this exact same renderer's own output on a component the
  resolver's dependency graph already resolved, never a raw string from
  outside the system, and a composition "cycle" is enforced to be a
  dependency cycle (same detector, same `E-RESOLVE-CYCLE`), not a new
  unbounded-recursion risk.
- **Dev server path safety.** `crates/omni-cli/src/http.rs` resolves every
  request path through `safe_join` against the serve root; a path that
  would escape gets `400`, not the file.
- **Safe out-of-process adapter invocation**
  (`crates/omni-adapter-api/src/process.rs`), used for `python-backend`:
  - **No shell.** Every spawn is `Command::new(executable).arg(script)` —
    an explicit executable and an explicit argument array, never a
    constructed shell string.
  - **Controlled working directory.** Always the already-validated
    project root.
  - **Controlled environment.** `.env_clear()` then a fixed allowlist
    (`PATH`, `SystemRoot`, `SystemDrive`, `TEMP`, `TMP`, `HOME`,
    `USERPROFILE`) — never the parent's full environment, never secrets
    (`controlled_env_never_leaks_arbitrary_variables` test).
  - **Enforced timeout.** The parent polls with `try_wait()` and calls
    `Child::kill()` past the deadline
    (`execute_times_out_on_a_hanging_process` test) — a hung or
    misbehaving adapter process cannot block a build indefinitely.
  - **Every protocol violation is a `Diagnostic`, not a panic or an
    assumed success**: empty response, malformed JSON, wrong protocol
    version, mismatched request id, an artifact path that resolves
    outside the output directory. Each has its own error code and its
    own test (`crates/omni-adapter-api/src/process.rs::tests`).
  - **Toolchain resolution has no silent failure mode.** A missing
    interpreter is `E-ADAPTER-TOOLCHAIN-MISSING`, naming every candidate
    tried, never a quietly-empty build.
- **Artifacts are re-verified, not trusted on an adapter's say-so.** After
  *any* adapter (in- or out-of-process) reports success, the orchestrator
  re-reads every claimed artifact from disk and hashes it
  (`orchestrator::artifact_record`); a claimed artifact that can't be
  re-read fails the whole target (`E-BUILD-ARTIFACT-VERIFY`).
- **No secrets in output.** Build reports, caches, and logs contain only
  file paths, sizes, SHA-256 hashes, and diagnostic text derived from
  project files — never environment variables or arbitrary process
  output. `omni doctor`/`adapter list` only print adapter-declared tool
  names and their own `--version` output, nothing else from the
  environment.
- **Deterministic, pinned dependencies.** `Cargo.lock` is committed; CI
  builds against the locked versions.
- **No implicit adapter execution.** Only adapters
  `crates/omni-cli/src/registry.rs` explicitly constructs ever run —
  today `web-static` (in-process) and `python-backend`/`node-backend`
  (out-of-process, but still scripts embedded in this binary at compile
  time, not discovered on disk or downloaded). Discovering an `adapter.yaml` on
  disk never, by itself, causes anything to be executed — see
  [ADR 0002](decisions/0002-in-process-adapters.md) and
  [ADR 0005](decisions/0005-out-of-process-python-backend.md).

## Real external tool invocation (`android-gradle`)

`android-gradle`'s `compile()` genuinely invokes a real, locally-discovered
Gradle (see [ADR 0011](decisions/0011-android-gradle-adapter.md)) via a
new, generic primitive —
`omni_adapter_api::process::run_tool`/`controlled_env_with` — reviewed
against the same checklist as `process.rs`/`devserver.rs`:

- **No shell** — `Command::new(gradle_exe).args(["assembleDebug",
  "--no-daemon"])` only; no constructed shell string, no `--project-dir`
  argument either (see ADR 0011 for why that specific argument was
  avoided on Windows).
- **Controlled environment** — `controlled_env_with` is the same
  `controlled_env()` allowlist plus exactly `JAVA_HOME`/`ANDROID_HOME`/
  `ANDROID_SDK_ROOT`, set to the paths this adapter's own toolchain
  discovery found — never the parent's full environment, never secrets.
- **Controlled working directory** — a scratch directory under this
  project's own `.omniforge/android-build/<target>/`, not a path derived
  from any build input.
- **Bounded timeout** — 10 minutes, generous because a cold Android
  Gradle Plugin/Kotlin plugin resolution genuinely can take a few
  minutes, but still enforced via the same poll-then-kill pattern
  `process::execute` uses, so a hung Gradle daemon cannot block a build
  indefinitely.
- **Artifact re-verification** — a claimed-successful Gradle exit is not
  trusted alone: the produced APK is opened as a real zip archive and
  checked for a nonempty `AndroidManifest.xml` entry before being copied
  into `dist/` and reported as a build success.

This is the same trust boundary as `python-backend`: Gradle, the Android
Gradle Plugin, and the Kotlin Gradle Plugin all run with the same
privileges as the `omni` CLI itself. No sandboxing beyond
argv/environment/working-directory/timeout control exists here either.
Unlike `python-backend`'s script (embedded in this repository), Gradle
and its plugins are **not** embedded — they are resolved from Google's/
Maven Central's real package repositories the first time a version isn't
already cached, which means (unlike every other adapter in this project)
`android-gradle` can, on a cold cache, perform real network access during
a build. This is disclosed here explicitly rather than left implicit.

## `omni dev`'s long-lived process management (production, not test-only)

`crates/omni-adapter-api/src/devserver.rs::ManagedProcess` is what `omni
dev` uses to start and stop a real, long-lived backend dev server (see
[dev-workflow.md](dev-workflow.md) and
[ADR 0009](decisions/0009-unified-dev-process-lifecycle.md)). It was
promoted from what was originally a test-only harness
(`crates/omni-cli/tests/support/mod.rs`, which now delegates to it), and
is reviewed against the same checklist as `process.rs`:

- **No shell** — `Command::new(exe).args(..)` only.
- **Controlled environment** — reuses
  `omni_adapter_api::process::controlled_env()`, the exact same allowlist
  `process.rs` uses; never the parent's full environment, never secrets.
- **Path safety** — the entry script/config paths come from the build's
  own already-verified artifact list and output directory, not
  user-suppliable strings.
- **Bounded startup wait** — a readiness-line read has an explicit
  timeout (`E-DEVSERVER-READY-TIMEOUT`); nothing here can hang `omni dev`'s
  startup.
- **Real cleanup, with a real attempt at graceful shutdown.** `stop(grace)`
  sends a platform-appropriate graceful signal first — `SIGTERM` on Unix
  (`libc::kill`), `CTRL_BREAK_EVENT` on Windows (`GenerateConsoleCtrlEvent`,
  targeted at the child's own process group via `CREATE_NEW_PROCESS_GROUP`
  so it cannot affect the `omni` process itself or its console) — waits up
  to the configured grace period, then falls back to a hard kill
  (`SIGKILL`/`TerminateProcess`). `Drop` always hard-kills and reaps if the
  process is somehow still alive, so a panic in `omni-cli` can't leak an
  orphan server process. Verified for real on both the Unix graceful path
  (a script that installs `SIGTERM`→exit cleanly stops gracefully; one
  that ignores it is hard-killed within the bound) and the Windows path
  (same two outcomes, using `CTRL_BREAK_EVENT` instead — see
  `crates/omni-adapter-api/src/devserver.rs`'s unit tests), and once more
  end-to-end against the real compiled `omni` binary with a genuine OS
  `CTRL_BREAK_EVENT` delivered from outside the process.

This is **process isolation, not a security sandbox** — same statement as
below for `python-backend` itself, and for the same reason: the script
being started is this repository's own reviewed example code (or a
project's own backend component), not third-party input.

## What's explicitly NOT implemented (don't assume otherwise)

- **`safe_join` does not resolve a pre-existing symlink placed inside
  `base`.** Confirmed with a real adversarial test, not a hypothetical:
  `crates/omni-core/src/paths.rs::tests::
  safe_join_does_not_resolve_a_pre_existing_symlink_inside_base` creates
  a real symlink inside a trusted base directory pointing outside it,
  calls `safe_join` (which accepts the resulting path — it is purely
  lexical, performing no `fs::canonicalize`/symlink resolution), then
  performs a real file write through the returned path and confirms it
  actually lands outside `base`. `OutputDir::resolve` *does* canonicalize
  and verify containment, but only once, for the output root itself — a
  symlink planted inside that root by an earlier write is not re-detected
  on every subsequent `.join()`. This is a narrow, same-privilege-level
  risk: an attacker who can already place such a symlink inside your
  project already has your own file-write access, so this is not a
  privilege escalation — but it is a real, confirmed gap, not a
  theoretical one, and is stated as such here and in `paths.rs`'s own
  module doc rather than relying on the previous (slightly overclaiming)
  wording that implied `safe_join` resolved symlinks generally.
- **No sandboxing of adapter child processes.** `python-backend`'s child
  process (and `android-gradle`'s real Gradle invocation, and everything
  Gradle itself spawns/downloads during a build) runs with the same user
  privileges as the `omni` CLI itself — argv/env/cwd control and a
  timeout, but no seccomp, no container, no capability drop. This is
  honest because every adapter that exists is one this repository ships
  and reviews, not third-party code (see below) — but it means a process
  boundary alone is not being sold as a security boundary here. For
  `android-gradle` specifically, this extends to Gradle's own plugin
  resolution: the Android Gradle Plugin and Kotlin Gradle Plugin are
  fetched from Google's/Maven Central's real repositories (not reviewed
  or pinned by hash in this repository beyond a version number), exactly
  the same trust model as running `gradle build` yourself would have.
- **No trust/enablement policy for third-party adapters.** Nothing in
  OmniForge today can load or run an adapter this repository didn't ship.
  See `docs/adapter-protocol.md`'s "Trust and enablement policy" section
  for the planned design once that becomes relevant.
- **No dependency audit tooling**, despite `security.audit_dependencies`
  existing as a config field. It is stored and validated but not yet
  wired to any check (e.g. `cargo audit` for this repo, or an equivalent
  for a future adapter's own dependencies — `python-backend` currently
  has none, being stdlib-only). Tracked as Planned.
- **No artifact signing.** Build reports include SHA-256 hashes for
  integrity *comparison*, not cryptographic signing/provenance.
- **No secret-scanning of user content.** If a user's own
  `component.yaml`/template/backend script contains a secret, OmniForge
  will happily render or run it — this tool does not try to detect
  secrets in a user's own source, the same way a compiler doesn't.
- **The backend adapter's captured output is not sandboxed from the
  component script's perspective.** `python-backend` runs the component's
  own script with the same controlled environment and the project root as
  cwd — appropriate for "a small backend application a developer wrote
  and is building with OmniForge," not for running arbitrary/untrusted
  scripts. This is the same trust boundary as running `python
  my_script.py` yourself; OmniForge does not add isolation beyond it.

## Reporting

See [SECURITY.md](../SECURITY.md).
