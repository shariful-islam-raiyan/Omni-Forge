//! Wires the `go-backend` adapter (`adapters/go-backend/`) into a
//! `ProcessAdapter` — mirrors `node_backend.rs`/`typescript_backend.rs`.
//! See docs/decisions/0015-go-backend-adapter.md.

use omni_adapter_api::ProcessAdapter;
use omni_core::spec::adapter::AdapterManifest;
use std::time::Duration;

const ADAPTER_MANIFEST_SRC: &str = include_str!("../../../../adapters/go-backend/adapter.yaml");
const ADAPTER_SCRIPT_SRC: &str = include_str!("../../../../adapters/go-backend/adapter.js");
const SCRIPT_FILENAME: &str = "adapter.js";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

pub fn manifest() -> AdapterManifest {
    let yaml: serde_yaml::Value = serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded go-backend adapter.yaml is valid YAML");
    let json: serde_json::Value =
        serde_json::to_value(&yaml).expect("yaml->json for go-backend manifest");
    let errors = omni_core::schema::validate(
        omni_core::schema::SchemaKind::Adapter,
        &json,
        "adapters/go-backend/adapter.yaml",
    );
    assert!(
        errors.is_empty(),
        "embedded go-backend adapter manifest failed schema validation: {errors:?}"
    );
    serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded go-backend adapter.yaml parses into AdapterManifest")
}

pub fn build() -> ProcessAdapter {
    build_with_candidates(omni_adapter_api::known_aliases("node"))
}

/// Exposed so tests can force toolchain-resolution failure deterministically
/// without relying on the test machine actually lacking Node.
pub fn build_with_candidates(launcher_candidates: Vec<String>) -> ProcessAdapter {
    ProcessAdapter::new(
        manifest(),
        launcher_candidates,
        SCRIPT_FILENAME,
        ADAPTER_SCRIPT_SRC,
        DEFAULT_TIMEOUT,
    )
}

pub fn script_source() -> &'static str {
    ADAPTER_SCRIPT_SRC
}

pub fn script_filename() -> &'static str {
    SCRIPT_FILENAME
}
