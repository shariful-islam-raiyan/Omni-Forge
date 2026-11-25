# Contributing to OmniForge

Thanks for your interest in contributing. OmniForge is a young,
language-neutral build-orchestration project — issues, bug reports, and
pull requests are welcome via
[GitHub Issues](https://github.com/shariful-islam-raiyan/Omni-Forge/issues)
and [Pull Requests](https://github.com/shariful-islam-raiyan/Omni-Forge/pulls).

## Development setup

Clone the repository (not `cargo install` — that's for *using* the CLI; see
[README.md](README.md#installation) for that) and build from source:

```sh
git clone https://github.com/shariful-islam-raiyan/Omni-Forge.git
cd Omni-Forge
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/verify.sh   # fmt + clippy + tests + end-to-end example builds
```

Requires a stable Rust toolchain (see `rust-toolchain.toml`) and, for the
`backend` target and its tests, a Python 3 interpreter on `PATH`
(`python3`/`python`/the Windows `py` launcher) — run `omni doctor` to check.

The optional, additive real-browser suite (`integration-tests/browser/`)
needs Node.js 18+ and is not required for most contributions — see
[its README](integration-tests/browser/README.md) if you're touching
`omni dev` or the full-stack example.

## Ground rules

- The core (`omni-core`, `omni-adapter-api`) must stay language-neutral: no
  parsing of JavaScript/Python/Dart/Rust/COBOL source for ordinary
  orchestration. Language-specific logic belongs in an adapter crate.
- Every new capability must be reflected honestly in `docs/roadmap.md`
  (Implemented / Partial / Planned / Blocked) — do not mark something
  implemented until it has a passing test.
- Adapters declare capabilities explicitly (`crates/omni-adapter-api`); the
  core must check capabilities before dispatching an operation, never assume.
- New adapters should pass the conformance suite in
  `crates/omni-adapter-api/src/conformance.rs`.
- Do not add dependencies without a clear need; prefer the standard library.

## Commit style

Small, focused commits. Describe *why*, not just *what*. One logical
change per pull request — if a feature is large, split it into
independently reviewable/testable pull requests rather than one giant
diff.

## Reporting issues / asking questions

Open a [GitHub Issue](https://github.com/shariful-islam-raiyan/Omni-Forge/issues).
For security vulnerabilities, see [SECURITY.md](SECURITY.md) instead of a
public issue.
