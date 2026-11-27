# OmniForge — Implementation Status

Maintained for session continuity across this continuous-execution
assignment. This file is a snapshot updated as milestones land — treat
`docs/roadmap.md` as the authoritative, item-by-item status source;
this file exists to make "what's done, what's next" visible without
reconstructing it from git log.

**Last updated:** 2026-11-27, after commit `aaae489`.

## Repository

- Path: `D:\Omni Forge`
- Branch: `main`
- Remote: `https://github.com/shariful-islam-raiyan/Omni-Forge.git` — in sync
- Working tree: clean after each committed milestone

## Local toolchain inventory (verified by direct probing, not assumed)

| Toolchain | Available | Notes |
|---|---|---|
| Rust / Cargo | Yes | 1.97.1, required for every target |
| Node.js / npm | Yes | v24.16.0 / 11.13.0 |
| Python | Yes, via `py` launcher | 3.14.0; bare `python`/`python3` are broken Store aliases on this machine |
| Android SDK/JDK/Gradle | Yes, via Android Studio's default install paths | Not on PATH directly; discovered by `omni-android-adapter`'s own toolchain detection |
| TypeScript (`tsc`) | No *global* install | Resolved on demand via an `npx`-equivalent (see ADR 0013); real builds succeed anyway |
| Go | Installation attempted via `winget` mid-session | See `IMPLEMENTATION_PROGRESS.md` for the outcome |
| Java/Gradle/GCC/Clang/MSVC (standalone, not via Android Studio) | No | Not on PATH |
| GnuCOBOL | No | Not installed |
| Xcode / macOS / iOS toolchain | N/A | This machine is Windows; genuinely cannot be verified here |

## Milestones completed this continuous-execution session

1. `node-backend` adapter (commit `21ed9fb`) — second out-of-process
   `backend` adapter, real Node execution, fixed a real Windows
   `\\?\`-path bug in `omni_adapter_api::process` along the way.
2. Parallel build scheduling across targets (commit `65aeef3`) —
   `omni.yaml`'s `build.parallel: true` now genuinely builds independent
   targets on separate OS threads; default stays sequential.
3. `typescript-backend` adapter (commit `ce3dbd0`) — real `tsc`
   type-checking (not just transpiling), proven by a dedicated test that
   a genuine type error fails the build. Fixed a real Windows `.cmd`-spawn
   bug (Node's hardened `child_process`) without introducing `shell: true`
   anywhere.
4. Implementation tracking files (commit `acc99fa`) — this file and its
   three companions.
5. SEO foundations for `web-static` (commit `bab936d`) — real, optional
   `seo:` manifest block producing metadata/canonical/Open Graph/
   `robots.txt`/`sitemap.xml`/minimal JSON-LD, plus `omni seo check`/
   `omni seo report` commands. Honestly scoped to the adapter's real
   one-page output — see ADR 0014 and `docs/seo.md`.
6. Real adversarial security tests + a doc-accuracy fix (commit
   `4154727`) — a genuine symlink-escape test against `safe_join`
   (confirmed real on this machine, not hypothetical) that corrected an
   overclaiming doc comment, plus a zip-entry path-safety regression test
   for `omni package`.
7. CI matrix expansion (commit `aaae489`) — added `macos-latest` to the
   main test job, added Node.js to that same job (previously only the
   browser-e2e job had it) so `node-backend`/`typescript-backend` tests
   run for real there, and added explicit build/verify steps for
   `hello-node`/`hello-typescript`/`seo check`. Configured and validated
   as syntactically correct YAML; **not yet observed to actually run** —
   this environment cannot trigger or watch a GitHub Actions run.

See `CHANGELOG.md` for full per-milestone detail and `docs/decisions/`
(ADRs 0012–0014) for the design rationale behind each.

## Test baseline

`cargo test --workspace`: **165 passing, 0 failed** (grew from 138 at the
start of this session). `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
clean. `bash scripts/verify.sh`: passes end to end, including real builds
of every example (`hello-web`, `hello-backend`, `hello-node`,
`hello-typescript`, `hello-fullstack`, `hello-android` — a real Android
APK — and `navbar-demo`), plus a real `omni seo check web` pass.

## What is genuinely NOT done (see `IMPLEMENTATION_BLOCKERS.md` for why)

- Go (attempt in progress this session — see `IMPLEMENTATION_PROGRESS.md`),
  Rust-application, C, C++, Java, Kotlin, Dart, Swift, COBOL adapters:
  not complete. `ProcessAdapter` is proven reusable (three real adapters
  now use it); the rest specifically need toolchains not present on this
  machine.
- iOS, Windows desktop, Linux desktop, macOS desktop adapters: not
  started. Genuinely need platforms/toolchains this machine doesn't have.
- Adapter SDK as a separate crate/scaffolding command (`omni adapter
  create`): not started.
- Linux/macOS-specific CI jobs actually exercising the Android/iOS/
  desktop story: not started beyond the matrix expansion in milestone 7
  above.

## Next concrete task

See `IMPLEMENTATION_PROGRESS.md`'s "In progress / next up" section.
