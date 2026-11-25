# ADR 0002: In-process adapters for v0.1, protocol specified but unused

## Status
Accepted for v0.1. **Revisited in
[ADR 0005](0005-out-of-process-python-backend.md):** the out-of-process
mechanism this ADR deferred is now real (`python-backend`), but the
*third-party adapter trust policy* this ADR also deferred is still not
implemented — that part of this decision still stands.

## Context
The master spec calls for an adapter *protocol* (versioned JSON messages
over stdin/stdout) so third-party, out-of-process adapters are possible,
but also explicitly requires: don't execute an adapter merely because a
manifest exists, require an explicit trust policy, don't claim full
sandboxing when only basic subprocess controls exist.

Building a real subprocess execution path *and* an honest trust/sandbox
story in the same milestone as the first working build would have meant
either (a) running untrusted-shaped code with no real isolation — a false
security story — or (b) building a sandbox, which is its own project.

## Decision
`web-static` implements the `Adapter` trait directly, compiled into the
`omni` binary (`crates/omni-cli/src/registry.rs`). The wire protocol
(`crates/omni-adapter-api/src/protocol.rs`, `docs/adapter-protocol.md`) is
fully specified and documented now, so a second adapter can be designed
against a reviewed format, but nothing in this repository serializes those
messages over a real pipe yet.

## Consequences
- Today's security story is simple and honest: there is no third-party
  adapter loading, so there is nothing to sandbox. `SECURITY.md` says this
  plainly.
- `omni adapter list` / `omni doctor` only ever show statically-linked
  adapters — never a claim of "discovered" adapters that aren't real.
- When a second (likely out-of-process) adapter is built, the trust policy
  decision (explicit user enablement per adapter, like a lockfile entry)
  still needs to be made and implemented — tracked in
  [docs/roadmap.md](../roadmap.md) as Planned, not Implemented.
