# ADR 0015: A real Go adapter (`go-backend`), compiling to a native executable

## Status
Accepted.

## Context
Go was explicitly named as a mandatory language adapter. Unlike
`node-backend`/`typescript-backend` (both run through an interpreter), Go
compiles to a real, standalone native executable with no runtime needed
to execute it — a genuinely different adapter shape, not just a fourth
copy of the same pattern. Go was not installed on this project's
development machine at the start of this milestone; it was installed via
`winget install --id GoLang.Go -e --silent` (the official GoLang
publisher, free, no credentials) mid-session, per this assignment's own
explicit permission to install a toolchain when safe and practical.

## Decision
`go-backend` (`adapters/go-backend/adapter.js`) keeps the same outer
shape as `node-backend`/`typescript-backend` — a Node.js wire-protocol
script, since `ProcessAdapter`'s launcher still needs an interpreter to
run the adapter *script itself* — but `validate`/`compile` both invoke a
real `go build`: `validate` builds to a discarded temp binary (a genuine
compile-check that writes nothing lasting, honoring the `Adapter` trait's
"`validate()` must not write any files" contract the same way
`typescript-backend`'s `tsc --noEmit` does); `compile` builds the real
executable directly into this build's `app/` output, then **executes
that binary directly** — no interpreter involved in running the
component's own code, unlike every other adapter so far. Proven
non-negotiably real by a dedicated test
(`crates/omni-cli/tests/go_backend_adapter.rs::
a_genuine_type_error_fails_both_validate_and_compile`): a real type error
(a string assigned to an `int`) fails the build with a genuine `go build`
diagnostic.

Two things fell out of building this against a real compiler:

- **A second real, general Windows environment-propagation bug** (after
  ADR 0012's `\\?\`-path bug and ADR 0013's `.cmd`-spawn bug): `go
  build`'s default build cache location on Windows is derived from
  `os.UserCacheDir()`, which specifically reads `%LocalAppData%`.
  `omni_adapter_api::process::controlled_env()`'s allowlist did not
  include it, so every `go build` invocation failed outright with "GOCACHE
  is not defined and %LocalAppData% is not defined" — reproduced on the
  very first real build attempt, not a hypothetical. Fixed by adding
  `LocalAppData` to the shared allowlist — general-purpose, benefiting
  any future tool following the same Windows convention, not Go-specific.
- **No `dev` block is declared.** The generic dev-serve convention
  (`docs/adapter-protocol.md`) assumes the same launcher that runs the
  adapter's own wire-protocol script also runs the build output
  (`<launcher> <app-entry> <run-config.json>`) — true for every adapter
  so far (all run their output through an interpreter), but false for a
  compiled Go binary, which must be executed directly as its own
  process. Supporting that properly needs a real change to
  `ProcessAdapter::dev_serve_plan`'s generic mechanism (letting an
  adapter's build output be its own launcher), not a per-adapter
  workaround — so `go-backend` simply omits `dev:` rather than declaring
  a capability it can't actually honor correctly. A real, tracked gap,
  not silently worked around.
- **`GO111MODULE=off`** is set for the `go build` invocation, so a
  single standalone `.go` file compiles without needing a `go.mod` —
  appropriate for this adapter's current scope (one source file per
  component, same granularity every other backend adapter here uses).
  A future module-aware mode (a whole package/module as the component
  source) is a natural extension, not attempted in this milestone.

## Consequences
- A fourth adapter now reuses the wire protocol unchanged for its outer
  layer, while being architecturally distinct in its inner layer
  (compile-to-native vs. interpret) — proof the protocol doesn't assume
  "interpreted language" even though every adapter before this one
  happened to be one.
- The `LocalAppData` fix benefits `go-backend` today and any future
  Windows-targeting tool with the same user-cache-directory convention.
- `go-backend`'s behavior on macOS/Linux is implemented the same way
  (platform-conditional executable naming, no Windows-specific
  verbatim-path special-casing needed since Go's own toolchain doesn't
  share Node's `.cmd`-spawn restriction) but **not verified on an actual
  macOS/Linux machine** — this project's only real development machine
  is Windows, flagged the same honest way every other adapter's
  macOS/Linux gap is flagged.
- `examples/hello-go` is real, buildable, and its artifacts (a genuine
  native `.exe`/ELF binary, run directly by `run.sh`/`run.bat`) were
  inspected directly, not merely assumed from a successful exit code.
