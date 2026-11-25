# ADR 0007: the backend dev-server is a documented example convention, not a new adapter operation

## Status
Accepted. **Superseded in part by [ADR 0009](0009-unified-dev-process-lifecycle.md):**
this ADR's *context* (why a generic `Run` wire-protocol operation is a
bigger design problem than the full-stack milestone needed to solve) is
still correct. Its *decision* — keeping the backend dev server a manual
step, with the test-only harness staying test-only and `omni dev` not
changing — was superseded once a later milestone needed exactly that: see
ADR 0009 for the capability-based (not wire-protocol) way it was finally
wired into `omni dev` for real. Kept verbatim below as the historical
record of the original, narrower decision.

## Context
The full-stack milestone requires the Python backend to expose a real,
persistent HTTP endpoint that an integration test can call. But
`python-backend`'s `compile` operation
(`adapters/python-backend/adapter.py::run_backend_script`) is, by design
(ADR 0005), a **one-shot, run-to-completion, capture-stdout** model: it is
invoked with a bounded timeout and is expected to exit. `omni build` must
finish in finite time and must never leave a server process running as a
side effect of a build — so `compile` cannot be the thing that starts a
long-lived HTTP server.

The adapter protocol's `Operation` enum
(`crates/omni-adapter-api/src/protocol.rs`) already reserves `Run`,
`Debug`, and `Inspect` variants for future use, but none are implemented —
sending any of them today gets a structured
`E-PYADAPTER-UNSUPPORTED-OPERATION` response, not a crash (see
`docs/adapter-protocol.md`). Implementing a real `Run` operation properly
would mean deciding, for *every* adapter and *every* service kind, how a
build-time process (`omni`) hands off a long-lived child it does not
intend to supervise for its full lifetime, how `omni dev`/`omni build`
would track and stop it later, and what the wire protocol says about a
response that never comes back (a running server has no "done" message).
That is a real design problem, not a one-line addition, and this
milestone's actual goal — prove a frontend and backend can talk over a
documented contract — does not require solving it.

## Decision
Keep the adapter protocol, `ProcessAdapter`, and `omni build`/`omni dev`
exactly as they are. The new example's service script
(`examples/hello-fullstack/lib/components/svc.api/service.py`) is
**dual-mode** by its own convention, not by any OmniForge mechanism:

- Invoked the same way every python-backend component already is
  (`python service.py <run-config.json>`), it behaves exactly like
  `examples/hello-backend`'s script — prints, exits. This is what `omni
  build backend` actually runs; nothing about the build path changed.
- Invoked with an additional `--serve <port>` flag — a convention of this
  one script, not a protocol OmniForge understands — it starts a real
  stdlib `http.server` and blocks.

Starting/stopping that server for testing is handled by a small,
explicitly test-scoped harness
(`crates/omni-cli/tests/support/mod.rs::spawn_backend_server`), not by
`omni build`/`omni dev`/the wire protocol. The harness reuses
`omni_adapter_api::process::controlled_env()` (made `pub` for exactly this
reuse) so it shares the same environment-allowlist guarantee as the real
adapter mechanism, but it is otherwise independent test code, documented as
such.

## Consequences
- Zero risk to anything that currently works: `adapter.py`, `ProcessAdapter`,
  the `Operation` enum, `examples/hello-backend`, and every existing test
  are untouched.
- A generic "start and later stop a long-lived service" adapter operation
  remains **Planned** (`docs/roadmap.md`), named explicitly as the
  limitation this milestone worked around rather than solved. The `--serve`
  convention here is one script's choice, not a cross-adapter standard —
  a different backend adapter, or a different service component, is free
  to expose a long-running mode however it likes (or not at all).
- Starting the real server for day-to-day development is a manual step
  (`examples/hello-fullstack/README.md` documents the exact command), not
  something `omni dev backend` does. `omni dev`'s existing documented scope
  ("only meaningful for `web` today") did not need to change.
