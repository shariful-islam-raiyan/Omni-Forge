//! Wiring for adapters that aren't plain Rust (`omni-web-adapter` lives in
//! its own crate because it *is* Rust; everything here instead embeds a
//! non-Rust script and wraps it in `omni_adapter_api::ProcessAdapter`). See
//! `docs/decisions/0004-cli-lib-split.md` for why this lives in the CLI
//! crate rather than a crate of its own.

pub mod python_backend;
