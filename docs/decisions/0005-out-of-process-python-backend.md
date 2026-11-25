# ADR 0005: A real out-of-process adapter (`python-backend`)

## Status
Accepted.

## Context
ADR 0002 shipped `web-static` in-process and left the wire protocol
specified but unused, reasoning that building subprocess execution and an
honest trust story in the same milestone as the first working build would
force a dishonest security claim. The next milestone's explicit goal was
to prove OmniForge is "genuinely extensible beyond web-static" — which
means building the out-of-process path for real, not just describing it.

## Decision
Build the execution engine properly first
(`crates/omni-adapter-api/src/process.rs`): safe argv-only spawning, a
fixed environment allowlist, enforced timeouts via polling + `kill()`,
and strict protocol validation (empty response, malformed JSON, protocol
version mismatch, request-id mismatch, unsafe artifact paths — each its
own `Diagnostic` code, never a panic or an assumed success). Then
implement one real adapter against it: `python-backend`
(`adapters/python-backend/adapter.py`), a stdlib-only Python script that
validates a `service`-kind component's inputs, actually executes the
component's script with a real Python interpreter, captures its output,
and writes a runnable package (`app/`, `run.sh`, `run.bat`) plus
proof-of-execution artifacts (`output.txt`, `run-config.json`).

Two supporting decisions fell out of this:

- **The toolchain resolver tries multiple candidate names.** This
  project's own dev machine has `python`/`python3` on `PATH` as broken
  Windows "app execution alias" stubs (they print a Store-install prompt
  and exit non-zero) while the `py` launcher resolves to a real
  interpreter. `omni_adapter_api::known_aliases("python3")` →
  `["python3", "python", "py"]`, tried in order via
  `resolve_executable`. This is not a Windows-only concern — the same
  function is what makes `omni doctor` and the adapter itself correctly
  report "available" instead of a false negative. It directly fixed a
  real bug: before this change, `omni doctor` reported `python-backend`'s
  requirement as missing on this exact machine.
- **The backend entry file is JSON, not YAML.** `omni-core` never parses
  a target's `entry` file contents — only the adapter does. Since
  `python-backend` is intentionally stdlib-only (no PyYAML dependency to
  install), its entry format (`backend/service.json`,
  `omniforge.dev/backend-app/v1`) is plain JSON. This is adapter policy,
  not core policy; a different out-of-process adapter is free to choose
  differently.

## Consequences
- OmniForge now has two working adapters on two different execution
  models, proving the `Adapter` trait abstraction actually holds: the
  orchestrator in `crates/omni-cli/src/orchestrator.rs` did not change at
  all to support `python-backend`.
- The trust/enablement policy for *third-party* adapters (ADR 0002's
  still-open item) remains unimplemented. `python-backend` is shipped and
  reviewed as part of this repository, embedded at compile time — not
  discovered on disk or downloaded. This ADR does not change that
  boundary; see `docs/security-model.md`.
- A latent bug in `omni_core::diagnostics::Diagnostic` was found and
  fixed while building this: its optional fields lacked
  `#[serde(default)]`, so JSON that omitted an absent field (rather than
  writing it as explicit `null`) failed to deserialize. Every adapter
  that hand-writes JSON diagnostics (as `python-backend` does) would have
  hit this. Fixed in the same change, with a regression test
  (`crates/omni-core/src/diagnostics.rs::
  deserializes_from_json_that_omits_optional_fields`).
- A real, reproducible Windows file-locking flake was found and fixed
  while exercising this adapter end to end: `omni clean` run immediately
  after a build that had just written a `.py` file could fail with
  "being used by another process" (almost certainly real-time antivirus
  scanning of the freshly-written script). `OutputDir::remove_all` now
  retries briefly (`omni_core::paths::remove_dir_all_with_retry`) before
  surfacing a persistent failure.
