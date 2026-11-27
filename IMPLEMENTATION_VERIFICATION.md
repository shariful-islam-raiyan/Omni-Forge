# OmniForge — Implementation Verification Log

Exact commands run and exact results, for every milestone in this
continuous-execution session. No result below is invented — each was
actually executed in this environment; re-run any of them yourself to
check.

## Baseline (start of session, commit `21ed9fb`)

```
cargo test --workspace
```
→ 146 passing, 0 failed (confirmed identical to the prior session's
reported count before any new work began).

## After `node-backend` adapter + Windows path fix (commit `21ed9fb`)

Already landed before this session's continuous-execution phase began;
re-verified as the baseline above.

## After parallel build scheduling (commit `65aeef3`)

```
cargo build --workspace            # clean
cargo fmt --all -- --check         # clean
cargo clippy --workspace --all-targets --all-features -- -D warnings   # clean
cargo test --workspace             # 148 passing, 0 failed
```
New tests: `crates/omni-cli/tests/parallel_build.rs`
(`parallel_build_runs_two_independent_targets_concurrently_and_both_succeed`,
`parallel_build_isolates_one_targets_failure_from_the_other`) — the first
genuinely builds `web-static` + `node-backend` concurrently and checks
real file contents (`dist/web/index.html`, `dist/backend/output.txt`),
not just exit codes.

## After `typescript-backend` adapter (commit `ce3dbd0`)

```
cargo build --workspace            # clean
cargo fmt --all -- --check         # clean
cargo clippy --workspace --all-targets --all-features -- -D warnings   # clean
cargo test --workspace             # 156 passing, 0 failed
bash scripts/verify.sh             # all checks passed, including hello-typescript
```

Manual end-to-end verification against the real compiled CLI
(`target/debug/omni.exe`), not just `cargo test`:

```
cd examples/hello-typescript
omni check            → target 'backend': OK
omni build backend    → target 'backend': SUCCESS (7 artifact(s))
cat dist/backend/output.txt
  → [1/3] Hello from hello-typescript, built by OmniForge's typescript-backend adapter!
  → [2/3] ...
  → [3/3] ...
```

Negative verification (genuine type-checking, not transpile-only) —
appended a real type error to the example's `.ts` source, re-ran `omni
check`, reverted:

```
error E-TSADAPTER-TYPECHECK-FAILED: tsc reported type/compile errors for '...\service.ts'
  lib/components/svc.greeter/service.ts(38,7): error TS2322: Type 'string' is not assignable to type 'number'.
target 'backend': FAILED
```
Exit code 1, confirmed via `echo "EXIT:$?"`.

## GitHub synchronization, each milestone

| Commit | `git rev-parse HEAD` after push | `git rev-parse origin/main` | Match |
|---|---|---|---|
| `65aeef3` | `65aeef3d763df9942482b5b009086bf6dae330f8` | same | Yes |
| `ce3dbd0` | `ce3dbd080480559930a0c35e0b7721f724d1c272` | same | Yes |

## What "verified" does NOT mean here

- No macOS/Linux machine has run any of this; every "Windows verified"
  claim in `README.md`/`docs/roadmap.md` means exactly that and no more.
- `android-gradle`'s real APK build (exercised by `verify.sh`) depends on
  Android Studio's bundled SDK/JDK/Gradle already being installed on this
  specific machine — not re-verified on a clean machine this session.
- Timing/performance of parallel vs. sequential builds was not
  benchmarked — only correctness (same outputs, real concurrency, failure
  isolation) was verified. No throughput claim is made.
