# ADR 0004: `omni-cli` gets a `lib` target

## Status
Accepted.

## Context
`omni-cli` was bin-only (`src/main.rs`, no `src/lib.rs`). That was fine
while every test could go through either `omni-core`/`omni-adapter-api`'s
own lib targets or `assert_cmd` black-box runs of the compiled `omni`
binary (`crates/omni-cli/tests/cli.rs`).

Testing the Python backend adapter's failure modes exposed the limits of
black-box-only testing:

- **Missing toolchain.** The only way to black-box-test "Python isn't
  available" is to manipulate `PATH` for the child process. On Windows,
  `CreateProcess` still searches system directories regardless of `PATH`,
  so this is unreliable across platforms — exactly the kind of flaky test
  this project's testing philosophy rules out.
- **Protocol edge cases** (unsupported operation, unsafe component source
  path, the backend script's own non-zero exit / inner timeout) are
  easiest and most precisely tested by driving the real adapter script
  directly with a hand-crafted `AdapterRequest`, which requires calling
  `omni_adapter_api::process::execute` with the adapter's embedded script
  source — not available to a test that can only shell out to `omni`.

## Decision
Add `[lib] name = "omni_cli"` to `crates/omni-cli/Cargo.toml`, move the
command wiring and `clap` types into `src/lib.rs` as `pub`, and reduce
`src/main.rs` to `fn main() -> ExitCode { omni_cli::run() }`. Integration
tests under `crates/omni-cli/tests/` can now do both: black-box runs via
`assert_cmd::Command::cargo_bin("omni")` (unchanged, still the primary
coverage for user-facing CLI behavior) and white-box calls into
`omni_cli::adapters::python_backend::build_with_candidates(...)` and
`omni_adapter_api::process::execute(...)` for the cases above
(`crates/omni-cli/tests/python_backend_adapter.rs`).

## Consequences
- No behavior change for end users; `omni --help`/`--version`/every
  subcommand works identically.
- `python_backend::build_with_candidates` and `script_source()`/
  `script_filename()` are now public API of the `omni_cli` lib crate
  specifically so tests can construct failure scenarios deterministically.
  They are not intended for use outside this crate's own test suite.
- Future adapters that need the same kind of white-box testing have a
  precedent to follow rather than inventing another workaround.
