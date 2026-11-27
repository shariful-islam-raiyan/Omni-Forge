# ADR 0012: A second real backend-language adapter (`node-backend`)

## Status
Accepted.

## Context
ADR 0005 built the out-of-process execution mechanism
(`crates/omni-adapter-api/src/process.rs`) explicitly as a language-neutral
primitive, and `ROADMAP.md` named "a second real backend-language adapter
… reusing the existing `ProcessAdapter` mechanism" as the next highest-
priority item, specifically to prove that claim rather than leave it
theoretical. `ROADMAP.md` named Node.js/TypeScript or Go as candidates.
Node.js was chosen because it is the one additional toolchain actually
installed and verifiable on this project's own development machine today
(no Go toolchain is present here) — consistent with this project's rule
that "Implemented" requires a passing test against the real thing, not a
manifest entry.

## Decision
Implement `node-backend` (`adapters/node-backend/adapter.js`) as a direct
structural mirror of `python-backend`: the same wire-protocol handling,
the same path-safety checks (`is_within`, reimplemented with
`fs.realpathSync.native` + `path.relative` instead of
`os.path.commonpath`), the same input-merging rules, the same
runnable-package artifact layout, and the same diagnostic-code
conventions (`E-NODEADAPTER-*`/`W-NODEADAPTER-*` in place of
`E-PYADAPTER-*`/`W-PYADAPTER-*`). It uses only Node's built-in modules
(`fs`, `path`, `child_process`) — no `package.json`, no dependency to
install beyond the Node runtime itself, mirroring `python-backend`'s
stdlib-only constraint for the same reason (nothing to install beyond the
interpreter). The Rust-side wiring
(`crates/omni-cli/src/adapters/node_backend.rs`) is a near-verbatim copy of
`python_backend.rs`, and `node-backend` is registered in
`crates/omni-cli/src/registry.rs` alongside it. Both adapters declare the
`backend` target; a project chooses between them per component via
`implementations.backend.adapter`, not via a new target name — the
`backend`-vs-`web`/`android` distinction is about what kind of thing is
being built, not which language builds it.

Three things fell out of actually building this against a real Node
interpreter rather than just wiring the manifest:

- **A real, reproducible Windows bug**: `python-backend` and
  `android-gradle` both already receive canonicalized (`\\?\...`-prefixed)
  paths from `omni-core`'s `loader::find_project_root`/`safe_join`, and
  tolerate it fine. Node.js does not — spawning
  `node "\\?\D:\Omni Forge\...\adapter.js"` as this project's own build (on
  this project's own `D:\Omni Forge` checkout, whose directory name
  contains a space) failed with `Module._findPath`'s internal
  `realpathSync` throwing `EISDIR` on a path it mis-parsed down to just the
  drive letter (`D:`). Not a hypothetical edge case — it reproduced on the
  very first real end-to-end attempt. Fixed with a new
  `omni_adapter_api::process::display_for_child` helper that strips the
  verbatim prefix only at the point an already-resolved absolute path is
  handed to a child process's argv/cwd (`execute`, `run_tool`,
  `ProcessAdapter::dev_serve_plan`) — never at the path-safety layer itself,
  which still canonicalizes and checks containment exactly as before. This
  is a generic Windows-interop fix, not Node-specific; any other external
  program with the same intolerance benefits from it automatically.
- **Node's own timeout signaling needed care.** `child_process.spawnSync`'s
  `timeout` option sets *both* `result.error.code === "ETIMEDOUT"` and
  `result.signal` on a timed-out child — checking `result.error` first
  (the naive translation of Python's `try/except TimeoutExpired`) silently
  misreported every real timeout as a generic execution failure
  (`E-NODEADAPTER-EXECUTION-FAILED` instead of
  `E-NODEADAPTER-EXECUTION-TIMEOUT`). Fixed by checking for the
  timeout-specific combination first. Caught by
  `crates/omni-cli/tests/node_backend_adapter.rs::
  backend_script_inner_timeout_is_reported_without_hanging`, which failed
  honestly on the first run rather than being hand-waved.
- **`known_aliases("node")` needed a second candidate.** Debian/Ubuntu's
  `nodejs` package has historically shipped a binary named `nodejs`, not
  `node` (a long-standing naming conflict with an unrelated package). Added
  alongside the existing `python3`/`python`/`py` table in the same function,
  so `omni doctor` and the adapter's own toolchain resolution report this
  accurately on that platform too — not verified on an actual
  Debian/Ubuntu machine (this project's development machine is Windows),
  but the same honest-until-verified posture `android-gradle`'s macOS/Linux
  discovery already uses (ADR 0011).

## Consequences
- `omni-adapter-api` did not change its public `Adapter` trait, its wire
  protocol, or its orchestrator integration at all to support this second
  adapter — only `process.rs` gained the `display_for_child` fix above and
  one `known_aliases` table entry, both general-purpose, not
  `node-backend`-specific. This is the proof point ADR 0005 deferred: the
  out-of-process mechanism really does generalize beyond the one adapter
  that originally exercised it.
- A real, previously-latent cross-platform bug
  (`\\?\`-prefixed paths breaking a Node child process) is now fixed for
  every current and future out-of-process/tool-shelling adapter on
  Windows, not just `node-backend` — `python-backend` and `android-gradle`
  were simply lucky not to trigger it.
- `node-backend`'s macOS/Linux behavior (including the `nodejs` alias) is
  implemented but **not verified on an actual macOS/Linux machine** — this
  project's only real development machine is Windows. Flagged the same way
  ADR 0011 flags `android-gradle`'s equivalent gap, rather than claimed as
  verified.
- A TypeScript adapter (type-checking + compilation via `tsc`) is a natural
  next step reusing this exact pattern, deliberately not attempted in the
  same milestone — this project's own rule is to build one real thing at a
  time with real tests, not several adapters superficially at once.
