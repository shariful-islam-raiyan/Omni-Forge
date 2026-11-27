# OmniForge — Implementation Progress Log

Chronological log of this continuous-execution session's work. Newest
entries at the top. Each entry: what was attempted, what actually
happened, and the commit it landed in (if any).

## In progress / next up

- **SEO foundations for `web-static`** (sitemap.xml, robots.txt, basic
  metadata: canonical URL, meta description, Open Graph, viewport). Pure
  Rust, no new toolchain dependency — chosen as the next milestone
  because it's both a major explicit requirement and fully achievable and
  testable on this machine, unlike most of the remaining platform/language
  work. Not started as of the last `IMPLEMENTATION_STATUS.md` update.

## Completed

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
  installing new toolchains or faking verification.
- Each milestone: implemented, tested (including negative/adversarial
  tests), fmt+clippy clean, full `cargo test --workspace` + `verify.sh`
  run, documented (CHANGELOG, README status tables, docs/roadmap.md, a
  numbered ADR), committed separately, and pushed — per the assignment's
  own Part 2 continuous-execution loop.
