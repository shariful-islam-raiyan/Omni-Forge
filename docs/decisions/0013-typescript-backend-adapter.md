# ADR 0013: A real TypeScript adapter (`typescript-backend`), with genuine type-checking

## Status
Accepted.

## Context
`docs/roadmap.md` (written while building `node-backend`, ADR 0012) named
a TypeScript adapter as the natural next step on the same pattern. The
assignment driving this milestone explicitly required "type checking" as
a mandatory capability, not merely stripping type annotations and running
the result as JavaScript — the two are easy to conflate and only one of
them is honest to call a TypeScript adapter.

## Decision
`typescript-backend` (`adapters/typescript-backend/adapter.js`) is
structurally identical to `node-backend` (same wire protocol, same
path-safety checks, same input merging, same runnable-package artifact
layout) with one real difference: before ever running the component's
script, it invokes the real TypeScript compiler (`tsc`) against it.
`validate` runs `tsc --noEmit` (type-check only, writes nothing — honoring
the `Adapter` trait's "`validate()` must not write any files" contract for
real, not just by convention). `compile` runs `tsc` for real with `--strict`
enabled, capturing genuine compiler diagnostics, then executes the
compiled JavaScript with `node` exactly like `node-backend` does. Proven
non-negotiably real by
`crates/omni-cli/tests/typescript_backend_adapter.rs::
a_genuine_type_error_fails_both_validate_and_compile`: a component whose
source assigns a `string` to a `number`-typed binding fails both
operations with the real `tsc` diagnostic (`TS2322`) — if this test
passed, the adapter would just be a transpiler, not a type checker.

Three things fell out of building this against the real compiler rather
than describing it:

- **Windows has no shell-free way to invoke `tsc` or `npx` directly if
  TypeScript isn't already globally installed.** A global `tsc` (via
  `npm install -g typescript`) and the `npx` launcher are both `.cmd`
  shims on Windows. Node's own `child_process` module deliberately refuses
  to spawn a `.cmd`/`.bat` file without `shell: true` (a hardening fix for
  a real command-injection advisory, CVE-2024-27980) — and this project
  does not use `shell: true` anywhere, by design (see
  `docs/security-model.md`'s "No shell" rule, which this ADR does not
  weaken). The no-shell-safe equivalent, used here: invoke npm's own
  bundled `npx-cli.js` — a plain JavaScript file shipped alongside
  `node.exe` in every standard Windows Node.js install — directly with
  `node`, exactly the same mechanism this adapter script itself already
  is. Off Windows, a bare `tsc`/`npx` spawn cleanly without any of this,
  so the special case is Windows-only (`resolveTsc()` in `adapter.js`).
  Not verified on an actual macOS/Linux machine (this project's only real
  development machine is Windows) — flagged the same honest way ADR
  0011/0012 flag their own macOS/Linux gaps.
- **`omni doctor` cannot usefully check `tsc` availability the way it
  checks `python3`/`node`.** Its generic `requires` check is "try this
  exact executable name (plus known aliases) directly on PATH." That
  would report a false `MISSING` for `tsc` on any machine without a
  *global* TypeScript install — even though a real build still succeeds
  via the on-demand `npx`-equivalent fallback above (verified: this
  project's own development machine has no global `tsc`, and
  `hello-typescript` still builds for real). The adapter's manifest
  therefore declares only `requires: [node]`, not `tsc` — the honest
  signal for whether a *build* will work is the build/validate result
  itself, not a doctor check that cannot model the fallback. Found via a
  real regression: adding `tsc` to `requires` broke
  `crates/omni-cli/tests/cli.rs::
  adapter_list_and_doctor_run_without_a_project`, which asserts `omni
  doctor` exits 0 on this project's own dev machine — a real false
  negative, not a test that needed loosening.
- **`@types/node` isn't assumed installed.** Without it (and without the
  `dom` lib), `tsc --strict` would reject even trivial use of `process`,
  `require`, or `console` as "Cannot find name" errors — not because the
  component's own logic is wrong, but because of missing Node ambient
  types. Rather than require installing `@types/node` (another network
  dependency), the adapter passes a tiny, embedded ambient shim
  (`NODE_GLOBALS_SHIM` in `adapter.js`: `process`/`require` declared as
  minimal ambient types) alongside `--lib ES2020,DOM` (for `console`).
  Real type-checking of the *component's own* variables, parameters, and
  return types still happens in full — this only stands in for Node's own
  ambient declarations a real project should eventually install properly.

## Consequences
- A third adapter now reuses `ProcessAdapter`/the wire protocol unchanged
  — further confirms ADR 0012's claim that the mechanism generalizes.
- `typescript-backend` uses a longer default timeout (60s vs 30s for
  `node-backend`/`python-backend`) to leave real room for a cold-cache
  `tsc` resolution plus a genuine compile step, both real wall-clock time
  beyond a bare interpreter startup.
- `omni doctor` will not surface a `tsc`-specific line for this adapter;
  a user who wants to know whether TypeScript will actually work should
  run `omni build backend`/`omni check`, which exercises the adapter's
  real resolution logic. This is a real, documented trade-off (precision
  over false confidence), not an oversight.
- `examples/hello-typescript` is real, buildable, and includes a
  genuinely type-checked, non-trivial interface (`GreeterConfig`) —
  not a placeholder.
