# OmniForge — Implementation Status

Maintained for session continuity across this continuous-execution
assignment. This file is a snapshot updated as milestones land — treat
`docs/roadmap.md` as the authoritative, item-by-item status source;
this file exists to make "what's done, what's next" visible without
reconstructing it from git log.

**Last updated:** 2026-11-27, after commit `ce3dbd0`.

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
| Go | No | Not installed; `where go` finds nothing |
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

See `CHANGELOG.md` for full per-milestone detail and `docs/decisions/`
(ADRs 0012, 0013) for the design rationale behind each.

## Test baseline

`cargo test --workspace`: **156 passing, 0 failed** (grew from 138 at the
start of this session: +8 node-backend, +2 parallel-build, +8
typescript-backend). `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
clean. `bash scripts/verify.sh`: passes end to end, including real builds
of every example (`hello-web`, `hello-backend`, `hello-node`,
`hello-typescript`, `hello-fullstack`, `hello-android` — a real Android
APK — and `navbar-demo`).

## What is genuinely NOT done (see `IMPLEMENTATION_BLOCKERS.md` for why)

- Go, Rust-application, C, C++, Java, Kotlin, Dart, Swift, COBOL adapters:
  not started. `ProcessAdapter` is proven reusable (three real adapters
  now use it); these specifically need toolchains not present on this
  machine.
- iOS, Windows desktop, Linux desktop, macOS desktop adapters: not
  started. Genuinely need platforms/toolchains this machine doesn't have.
- SEO engine (sitemap/robots/metadata/structured data): not yet started
  as of this file's last update — see `IMPLEMENTATION_PROGRESS.md` for
  current in-flight work.
- Adapter SDK as a separate crate/scaffolding command (`omni adapter
  create`): not started.
- Cross-platform CI matrix expansion (macOS/Linux runners actually
  exercising their respective adapters): not started this session.

## Next concrete task

See `IMPLEMENTATION_PROGRESS.md`'s "Currently in progress" section.
