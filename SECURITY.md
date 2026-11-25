# Security Policy

## Reporting a Vulnerability

Please report security issues privately rather than opening a public
issue — use GitHub's private vulnerability reporting
(repository **Security** tab → **Report a vulnerability** at
https://github.com/shariful-islam-raiyan/Omni-Forge/security/advisories/new).
Include reproduction steps and the affected version (`omni --version`).

## Security Model Summary

See [`docs/security-model.md`](docs/security-model.md) for the full threat model.
In short, today:

- Adapters run either in-process (`web-static`) or as a child process
  spawned with safe argv, a controlled working directory, and a minimal
  environment allowlist (`python-backend`) — see
  `crates/omni-adapter-api/src/process.rs`. A build/check invocation is
  always short-lived and bounded by an enforced timeout; `omni dev` may
  additionally supervise a real long-lived dev-server process for the
  duration it runs, with a graceful-then-hard-kill shutdown (see
  `crates/omni-adapter-api/src/devserver.rs`). Either way, every adapter
  is one this repository ships and reviews; there is no untrusted
  third-party adapter loading.
- All filesystem writes are confined to a canonicalized output directory; path
  traversal and symlink escapes are rejected.
- `omni clean` never deletes anything outside the resolved, validated output
  directory.
- Component templates use escaped-substitution rendering only — there is no
  embedded script engine and no `eval`-like mechanism.
- No secrets are read, stored, or embedded in build reports, caches, or logs by
  the current implementation.

Known gaps (tracked in `docs/roadmap.md`): out-of-process adapter sandboxing,
dependency auditing, and artifact signing are not yet implemented.
