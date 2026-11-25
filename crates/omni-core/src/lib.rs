// `OmniError::Diagnostic` carries a `Diagnostic` by value (~184 bytes) for
// ergonomic `?`/`From` use at call sites; boxing it would add noise with no
// measurable benefit for a CLI tool's error paths.
#![allow(clippy::result_large_err)]

//! `omni-core`: language-neutral project model, validation, resolution, and
//! build-planning primitives shared by the CLI and every adapter.
//!
//! This crate must never parse application source code (JavaScript, Python,
//! Dart, Rust, COBOL, ...) to do ordinary orchestration — that belongs in an
//! adapter. It only understands the OmniForge specification documents
//! (`omni.yaml`, `component.yaml`, adapter manifests, build reports).

pub mod cache;
pub mod components;
pub mod diagnostics;
pub mod error;
pub mod fsutil;
pub mod graph;
pub mod loader;
pub mod paths;
pub mod resolver;
pub mod schema;
pub mod spec;

pub use diagnostics::{Diagnostic, Diagnostics, Severity};
pub use error::{OmniError, Result};
