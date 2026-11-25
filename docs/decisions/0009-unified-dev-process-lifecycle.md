# ADR 0009: `omni dev` becomes a real, unified, multi-target process-lifecycle runtime

## Status
Accepted. Supersedes [ADR 0007](0007-backend-dev-server-is-example-scoped.md)'s
scoping decision (ADR 0007's *context* — why a generic `Run` wire-protocol
operation is a bigger problem than this needed to solve — is still correct
and is not revisited here; only its *decision* to keep the backend dev
server manual/test-only is superseded).

## Context
Before this milestone, `omni dev <target> [--port]`
(`crates/omni-cli/src/commands/dev.rs`) only ever did a single bounded
build (`orchestrator::build_target`, the same call `omni build` makes) and
then served the output directory as static files — meaningful for `web`,
silently useless for `backend` (a backend build's output is a runnable
package, not a static site), and with no process-lifecycle concept at all:
no spawning, no supervision, no shutdown handling, not even a `Ctrl-C`
handler (the static server just blocked in an accept loop and relied on
the OS/terminal to kill the whole process).

Meanwhile a real, tested process-lifecycle pattern already existed —
`crates/omni-cli/tests/support/mod.rs::spawn_backend_server`/`ServerGuard`
— but was deliberately test-only (ADR 0007), because turning it into a
generic, protocol-level `Run` operation (deciding, for every adapter and
every service kind, how a long-lived child hands off, how `omni` tracks it
later, what the wire protocol says about a response that never comes) was
rightly judged too large for that milestone.

This milestone's explicit goal is a real unified dev workflow with genuine
process lifecycle management (startup/shutdown coordination, graceful
termination where supported, child cleanup, timeouts, port
configuration/conflict diagnostics) — which requires actually solving the
thing ADR 0007 deferred, at least for the case that already has a real,
working example (`python-backend`'s dev-serve convention).

## Decision
1. **Promote the process-lifecycle pattern into production**, generalized
   beyond the one test harness: `omni_adapter_api::devserver::ManagedProcess`
   (spawn with a readiness check and bounded timeout; `stop(grace)` attempts
   a graceful signal — `SIGTERM` on Unix, `CTRL_BREAK_EVENT` on Windows,
   both empirically verified on this milestone's dev machine and in
   `devserver`'s own unit tests — before falling back to a hard kill;
   `Drop` always reaps). The test harness now delegates to this instead of
   duplicating it.
2. **Add a capability, not a new wire-protocol operation.** Rather than
   implementing the `Operation::Run` ADR 0007 flagged as a bigger design
   problem, `adapter.yaml` gained an optional `dev` block (args template,
   stdout readiness-line prefix, timeouts) and `Adapter` gained a default
   `dev_serve_plan(report, output_dir) -> Option<Result<DevServePlan, _>>`
   method. This describes how to run an adapter's already-produced build
   *output* as a long-lived process — a packaging/invocation convention,
   not a new request/response message the wire protocol has to define a
   "never comes back" case for. `ProcessAdapter` implements it generically
   (no Python-specific code) against a documented "runnable-package
   convention" (see `docs/adapter-protocol.md`); `python-backend` is the
   one adapter that opts in today, reusing its already-shipped `--serve
   <port>` script convention.
3. **`omni dev` becomes unified**: builds every enabled (or named) target,
   starts each one the way its adapter supports (a managed child process,
   or — for in-process adapters like `web-static` — the existing static
   file server, now itself stoppable rather than relying on process exit),
   prints a `READY <target> <url>` line per target, and blocks for a real
   shutdown signal (`ctrlc`-backed Ctrl-C/Ctrl-Break in the real CLI; an
   injectable channel in tests) before stopping everything and exiting.
   A target-level failure (bad build, missing toolchain, no dev-serve
   support) is reported and skipped rather than aborting the whole
   session — `omni dev` is a best-effort "show me what's working" command,
   matching `omni build --allow-partial`'s existing philosophy.
4. **Ports**: default to an auto-picked free ephemeral port (no collision
   by construction); `omni.yaml`'s `targets.<name>.dev.port` or `--port
   NAME=PORT` can pin one. Two targets pinned to the *same* port is
   detected and reported (`E-DEV-PORT-CONFLICT`) before anything is
   spawned, never a confusing bind failure after the fact. A port already
   held by something unrelated still surfaces as a clear failure for that
   target, not a silent no-op.

## Consequences
- `omni dev backend` now actually starts a real server instead of
  (previously) uselessly serving a runnable package's files as if it were
  a static site.
- `crates/omni-cli/src/commands/dev.rs` changed its CLI shape (`target:
  String` → `targets: Vec<String>`, `--port <u16>` → repeatable `--port
  NAME=PORT`). No existing test exercised the old shape (confirmed by
  search before changing it), so this is not a tested-behavior break;
  README examples were updated to match.
- Verified for real on this milestone's dev machine, not just by
  construction: the compiled `omni` binary was run against
  `examples/hello-fullstack`, both targets came up, real HTTP requests
  against both succeeded, a genuine Windows `CTRL_BREAK_EVENT` was
  delivered to the process, and the backend child was confirmed gone
  (its port stopped accepting connections) after a clean exit — see the
  milestone's own verification notes.
- The generic `Operation::Run`/`Debug`/`Inspect` wire-protocol operations
  remain unimplemented and **Planned** — this ADR deliberately did not
  solve that larger problem, only the narrower, already-concrete one this
  milestone needed.
