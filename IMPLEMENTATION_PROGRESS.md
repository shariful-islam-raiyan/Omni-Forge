# OmniForge — Implementation Progress Log

Chronological log of this continuous-execution session's work. Newest
entries at the top. Each entry: what was attempted, what actually
happened, and the commit it landed in (if any).

## In progress / next up

- **Go toolchain installation attempt**, via
  `winget install --id GoLang.Go -e --silent` (official GoLang publisher,
  free, no credentials needed — matches the assignment's "if safe and
  practical" allowance). Running in the background as of this update; if
  it succeeds, the next real milestone is a `go-backend` adapter
  (compile-then-run, similar shape to `typescript-backend`'s `tsc`
  compile step, but producing a native executable rather than running
  through an interpreter). If it fails or doesn't complete in reasonable
  time, this is recorded honestly in `IMPLEMENTATION_BLOCKERS.md` rather
  than silently abandoned.

## Completed

### Milestone: CI matrix expansion — commit `aaae489`
Added `macos-latest` to the main CI job's matrix and Node.js to that same
job. Added explicit build/verify steps for `hello-node`/`hello-typescript`/
`omni seo check web`. Validated as syntactically correct YAML using the
project's own `serde_yaml` dependency before committing. Not yet observed
to actually execute — this environment has no way to trigger or watch a
GitHub Actions run.

### Milestone: real adversarial security tests — commit `4154727`
Added a real (not hypothetical) test that creates an actual symlink
inside a trusted base directory pointing outside it, proves `safe_join`
accepts the resulting path (it performs no symlink resolution), and
confirms a real file write through it lands outside the trusted base —
this directly contradicted `paths.rs`'s own module doc comment, which was
fixed to state the precise, narrower scope. Documented as a real,
confirmed, same-privilege-level residual risk in
`docs/security-model.md`. Also added a zip-entry path-safety regression
test for `omni package`. 165/165 tests pass.

### Milestone: SEO foundations — commit `bab936d`
Optional `seo:` block in the web app manifest producing real metadata
(description, canonical, Open Graph, robots), `robots.txt`,
`sitemap.xml` (only with `seo.site_url` configured — honestly skipped
otherwise), and a minimal `WebSite` JSON-LD block. New `omni seo check`/
`omni seo report` commands doing real string-based heuristic checks
against built HTML (documented as a heuristic, not a full parser).
Scoped honestly to `web-static`'s real one-page output — no claim of
multi-page SEO, SSR/SSG, or richer schema.org types without real backing
data. 163/163 tests pass at this point.

### Milestone: implementation tracking files — commit `acc99fa`
This file and its three companions, created per the assignment's
explicit request for session-continuity tracking.

### Milestone: `typescript-backend` adapter — commit `ce3dbd0`
Real `tsc --noEmit`/`tsc --strict` type-checking before execution, proven
by a test where a genuine `TS2322` type error fails the build. Found and
fixed a real Windows bug (Node refuses to spawn `.cmd` files without
`shell: true`, which this project never uses) by invoking npm's bundled
`npx-cli.js` directly with `node` instead. 156/156 tests pass.

### Milestone: parallel build scheduling — commit `65aeef3`
`omni.yaml`'s previously-inert `build.parallel` field now genuinely
builds independent targets on separate OS threads (`std::thread::scope`)
when `true`; default (`false`) is byte-for-byte unchanged. Proven with
two genuinely different, concurrently-running adapters
(`web-static` + `node-backend`). 148/148 tests pass at this point.

### Milestone: `node-backend` adapter — commit `21ed9fb`
Second out-of-process `backend`-target adapter (first new adapter added
in this continuous session, carried over from the previous session's
final commit). Found and fixed a real Windows bug: canonicalized
(`\\?\`-prefixed) paths broke Node's own module resolution for a child
process's main script. 146/146 tests pass at this point.

## Decisions made along the way (not asked, judgment calls — flagging for review)

- Given this mega-assignment's scope (12 language adapters, 6 platform
  adapters, full SEO/security engines, cross-OS CI — realistically a
  multi-week effort) vastly exceeds what a single session can honestly
  deliver with real toolchain-backed verification, chose to execute
  continuously through *real, individually verified* milestones in
  priority order (reusing the project's own stated `ROADMAP.md`
  priorities first) rather than attempt shallow/fake coverage of
  everything at once. This directly follows the assignment's own "no fake
  implementations" rule, which a breadth-first shallow pass would violate.
- Prioritized adapters/features buildable with the toolchains actually
  present on this machine (Rust, Node+npm, Python via `py`, Android via
  Android Studio's bundled SDK/JDK/Gradle) over ones that would require
  installing new toolchains or faking verification — then, once those
  were exhausted, attempted installing one more toolchain (Go) per the
  assignment's own explicit permission to do so when safe.
- Each milestone: implemented, tested (including negative/adversarial
  tests), fmt+clippy clean, full `cargo test --workspace` + `verify.sh`
  run, documented (CHANGELOG, README status tables, docs/roadmap.md, a
  numbered ADR where warranted), committed separately, and pushed — per
  the assignment's own Part 2 continuous-execution loop.
- Could not use `gh` CLI (not installed in this environment) to monitor
  the CI run this session's changes trigger — flagged explicitly rather
  than claiming "CI verified" without evidence.
