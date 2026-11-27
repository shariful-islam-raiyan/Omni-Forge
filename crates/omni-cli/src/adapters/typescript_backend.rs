//! Wires the `typescript-backend` adapter (`adapters/typescript-backend/`)
//! into a `ProcessAdapter` — mirrors `node_backend.rs` exactly, with a
//! longer default timeout (60s vs 30s) since a real `tsc` type-check/
//! compile step, and potentially an `npx` cold-cache resolution of the
//! TypeScript compiler, both take real wall-clock time beyond a bare
//! interpreter startup. See docs/decisions/0013-typescript-backend-adapter.md.

use omni_adapter_api::ProcessAdapter;
use omni_core::spec::adapter::AdapterManifest;
use std::time::Duration;

const ADAPTER_MANIFEST_SRC: &str =
    include_str!("../../../../adapters/typescript-backend/adapter.yaml");
const ADAPTER_SCRIPT_SRC: &str = include_str!("../../../../adapters/typescript-backend/adapter.js");
const SCRIPT_FILENAME: &str = "adapter.js";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

pub fn manifest() -> AdapterManifest {
    let yaml: serde_yaml::Value = serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded typescript-backend adapter.yaml is valid YAML");
    let json: serde_json::Value =
        serde_json::to_value(&yaml).expect("yaml->json for typescript-backend manifest");
    let errors = omni_core::schema::validate(
        omni_core::schema::SchemaKind::Adapter,
        &json,
        "adapters/typescript-backend/adapter.yaml",
    );
    assert!(
        errors.is_empty(),
        "embedded typescript-backend adapter manifest failed schema validation: {errors:?}"
    );
    serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded typescript-backend adapter.yaml parses into AdapterManifest")
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
