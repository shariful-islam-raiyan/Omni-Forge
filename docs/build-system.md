# Build system

`crates/omni-cli/src/orchestrator.rs::build_target` is the whole
orchestrator today. One call builds one target; `omni build all` calls it
once per enabled target, sequentially. Everything below applies
identically whether the resolved adapter runs in-process (`web-static`)
or out-of-process (`python-backend`) — the orchestrator calls
`adapter.validate()`/`.compile()` on a `Box<dyn Adapter>` either way and
has no branch for which kind it has.

## Steps, per target

1. Look up the target in `omni.yaml`. Not present → `Skipped` with
   `E-BUILD-UNKNOWN-TARGET`. Present but `enabled: false` → `Skipped` with
   an `info`-level `I-BUILD-TARGET-DISABLED` (this is not a failure).
2. Enabled but no `adapter` configured → `Failed`,
   `E-BUILD-MISSING-ADAPTER-CONFIG` (this should already have been caught
   by `omni check`/project loading, but the orchestrator re-checks rather
   than trusting the caller).
3. Configured adapter not present in this CLI build's registry →
   `Unsupported`, `E-BUILD-ADAPTER-UNAVAILABLE`. **This is the path that
   keeps `omni build all` honest**: an enabled target with no real adapter
   is reported as unsupported, never silently skipped and never counted
   as a success.
4. Compute roots (components implementing this target) and resolve them
   (see [resolver.md](resolver.md)). Any resolution error → `Failed` with
   the resolver's diagnostics.
5. Resolve the target's `entry` path (if configured) safely against the
   project root; resolve/create the per-target output directory
   `​<build.output>/<target>` as an `OutputDir` (canonicalized, verified
   inside the project root).
6. **Validate-only mode** (`omni check`): call `adapter.validate(ctx)` and
   stop — no cache, no files written beyond the (empty) output directory
   that `OutputDir::resolve` creates.
7. **Compile mode** (`omni build`): compute the cache key (see
   [caching.md](caching.md)); on a hit, restore the cached tree and report
   `Success`/`cache: hit`; on a miss, call `adapter.compile(ctx)`.
8. **Artifact verification**: every artifact the adapter reports is
   re-read from disk (size + SHA-256) via `fsutil::sha256_file`. If any
   claimed artifact can't be re-read, the whole target is `Failed` with
   `E-BUILD-ARTIFACT-VERIFY` — a successful-looking adapter result is not
   enough; the files have to actually be there afterward.
9. On success with caching enabled, the output tree is stored under the
   computed key for next time.

## `omni build <target>` vs `omni build all`

- `omni build web` builds exactly one target; any non-`Success` status is
  a process exit code of 1.
- `omni build all` builds every target with `enabled: true`. If any target
  is `Failed`, the whole command fails (exit 1) regardless of flags. If
  one or more enabled targets are `Unsupported` (no adapter) and
  `--allow-partial` was **not** passed, the command also fails (exit 1) —
  partial success is never the default. With `--allow-partial`, the
  command exits 0 but prints `build PARTIAL` and the written
  `build-report.json`'s overall `status` field reflects `partial`, never
  `success`.

## Build report (`dist/build-report.json`)

Schema: `specs/build-report.schema.json`. Written by `commands/build.rs`
after every `omni build` invocation (for the targets attempted in that
run). Fields: project name/version, RFC 3339 timestamp, overall `status`
(`success`/`failed`/`partial`, derived in
`BuildReport::new` from the per-target statuses — never set independently,
so it can't drift from what actually happened), and one `TargetReport` per
target with its adapter id/version, status
(`success`/`failed`/`partial`/`unsupported`/`skipped`), cache result
(`hit`/`miss`/`disabled`), the verified artifact list (path, byte count,
SHA-256), and every diagnostic produced.

No secrets, environment variables, or absolute-outside-project paths are
written into the report — see [security-model.md](security-model.md).

## Parallel target builds

`build_target` runs everything for *one* target sequentially (its own
component graph, in dependency order). Across *multiple* targets,
`commands::build` offers two paths, chosen by `omni.yaml`'s
`build.parallel` field:

- **`parallel: false` (default).** `omni build all`/an explicit target
  list runs targets one after another — unchanged since this project's
  first release.
- **`parallel: true`**, with more than one target to build: each target
  is built on its own OS thread (`std::thread::scope` in
  `crates/omni-cli/src/commands/build.rs`), joined, then results are
  printed and assembled into `build-report.json` in the *original*
  target-name order — never completion order — so the report stays
  deterministic regardless of which adapter actually finished first.
  Safe because independent targets cannot collide on disk: the build
  cache is scoped per target (`.omniforge/cache/<target>/...`, see
  [caching.md](caching.md)) and output is scoped per target
  (`dist/<target>/...`); the resolver already computes each target's
  component graph independently of any other target's. One target's
  failure cannot affect another's already-written artifacts or status
  either way — proven for both the sequential and parallel path by
  `crates/omni-cli/tests/build_all_partial_failure.rs` and
  `crates/omni-cli/tests/parallel_build.rs`.

This does **not** parallelize *within* one target's component graph
(`DependencyGraph::topological_order`'s per-component scheduling) — that
remains sequential and is still Planned, tracked in `docs/roadmap.md`.

## What's deliberately not here yet

- **Intra-target parallelism.** One target's own component graph still
  builds sequentially even under `build.parallel: true` — see above.
- **Incremental, sub-target caching.** The cache is keyed per target as a
  whole (see [caching.md](caching.md)), not per component or per file.
