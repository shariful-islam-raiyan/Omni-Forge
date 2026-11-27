# OmniForge — Implementation Status

Maintained for session continuity across this continuous-execution
assignment. This file is a snapshot updated as milestones land — treat
`docs/roadmap.md` as the authoritative, item-by-item status source;
this file exists to make "what's done, what's next" visible without
reconstructing it from git log.

**Last updated:** 2026-11-27, after commit `3d0411c`.

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
| TypeScript (`tsc`) | No *global* install | Resolved on demand via an `npx`-equivalent (ADR 0013); real builds succeed anyway |
| Go | **Yes, installed this session** | `winget install --id GoLang.Go`, official publisher; 1.27.4; needs a fresh shell session's PATH, or explicit `PATH="$PATH:/c/Program Files/Go/bin"` in this one |
| Java/Gradle/GCC/Clang/MSVC (standalone, not via Android Studio) | No | Not on PATH |
| GnuCOBOL | No | Not installed |
| Xcode / macOS / iOS toolchain | N/A | This machine is Windows; genuinely cannot be verified here |

## Milestones completed this continuous-execution session

1. `node-backend` adapter (`21ed9fb`) — second out-of-process `backend`
   adapter; fixed a real Windows `\\?\`-path bug.
2. Parallel build scheduling (`65aeef3`) — `build.parallel: true` now
   genuinely builds independent targets on separate OS threads.
3. `typescript-backend` adapter (`ce3dbd0`) — real `tsc` type-checking;
   fixed a real Windows `.cmd`-spawn bug without using `shell: true`.
4. Implementation tracking files (`acc99fa`).
5. SEO foundations for `web-static` (`bab936d`) — real metadata,
   `robots.txt`, `sitemap.xml`, `omni seo check`/`report`.
6. Real adversarial security tests (`4154727`) — a confirmed real
   symlink-escape against `safe_join`, fixing an overclaiming doc
   comment; a zip-entry path-safety regression test.
7. CI matrix expansion (`aaae489`) — `macos-latest` added, Node.js added
   to the main gate. Configured, not yet observed to run.
8. `docs/testing.md` accuracy fix (`1e564f9`).
9. `omni adapter validate` (`a1bed80`) — real schema+conformance
   validation for any adapter manifest on disk, no registration needed.
10. `go-backend` adapter (`48df1de`) — real `go build` to a native
    executable, run directly. Go installed mid-session via `winget`.
    Found and fixed a second Windows env-propagation bug (`%LocalAppData%`).
11. `windows-desktop` adapter (`3d0411c`) — **the first non-web/backend/
    android target.** Real `rustc` → a real native PE32+ executable, run
    directly. New crate `omni-windows-adapter`. Found and fixed a real
    gap: `validate()` wasn't actually invoking the compiler.

See `CHANGELOG.md` for full per-milestone detail and `docs/decisions/`
(ADRs 0012–0016) for the design rationale behind each.

## Test baseline

`cargo test --workspace`: **188 passing, 0 failed** (grew from 138 at the
start of this session). `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
clean. `bash scripts/verify.sh`: passes end to end, including real builds
of every example (`hello-web` + `omni seo check`, `hello-backend`,
`hello-node`, `hello-typescript`, `hello-go`, `hello-windows`,
`hello-fullstack`, `hello-android` — a real Android APK — and
`navbar-demo`).

## Adapters now real and tested (7 total, 3 architectural styles)

| Adapter | Target | Style | Real toolchain |
|---|---|---|---|
| `web-static` | `web` | in-process | none |
| `python-backend` | `backend` | out-of-process | Python 3 |
| `node-backend` | `backend` | out-of-process | Node.js |
| `typescript-backend` | `backend` | out-of-process | Node.js + `tsc` |
| `android-gradle` | `android` | in-process, shells out | Gradle/SDK/JDK |
| `go-backend` | `backend` | out-of-process, compiles to native | Go |
| `windows-desktop` | `windows` | in-process, shells out, compiles to native | `rustc` |

## What is genuinely NOT done (see `IMPLEMENTATION_BLOCKERS.md` for why)

- C, C++, Java, Kotlin, Dart, Swift, COBOL, Rust-as-a-backend adapters:
  not started — need toolchains not present on this machine.
- iOS, Linux desktop, macOS desktop adapters: not started — need an OS
  this machine does not have, not merely a missing install.
- Adapter SDK as a separate scaffolded crate/`omni adapter create`
  (project *generation*, as opposed to `omni adapter validate`'s
  validation, which is done): not started.
- A real GUI surface for `windows-desktop` (a Win32 message box or
  window): architecturally compatible, not attempted — see ADR 0016.
- Component system advanced features (typed outputs actually consumed
  across components, event contracts, multiple-implementation
  selection): still Partial, unchanged this session — deliberately not
  attempted without being able to validate a new syntax/semantics design
  against user intent first.
- Linux/macOS-specific CI jobs actually exercising the Android/desktop
  story beyond the matrix expansion already done: not started.

## Next concrete task

No single next task is queued — the highest-value remaining work
(additional language/platform adapters) is blocked on toolchains/OSes
this machine doesn't have. See `IMPLEMENTATION_BLOCKERS.md`'s "what would
unblock each" column for the concrete next step per item.
