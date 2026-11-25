## What this changes and why

## Checklist

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace` (and the new/changed tests actually fail without this change)
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `bash scripts/verify.sh` (or the Windows-equivalent manual run)
- [ ] `docs/roadmap.md` updated if this changes a feature's status
- [ ] `CHANGELOG.md` updated
- [ ] No secrets, credentials, or local-machine paths committed

## Related issue(s)

Closes #
