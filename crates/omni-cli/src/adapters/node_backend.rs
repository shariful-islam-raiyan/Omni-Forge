//! Wires the `node-backend` adapter (`adapters/node-backend/`) into a
//! `ProcessAdapter`. The manifest and script are embedded at compile time
//! (so the `omni` binary is self-contained — no separate install step) and
//! materialized into `.omniforge/adapters/node-backend/` on first use. This
//! mirrors `python_backend.rs` exactly — `ProcessAdapter` itself has no
//! Python-specific code, so a second out-of-process adapter is just a
//! different manifest + script, no changes to `omni-adapter-api`.

use omni_adapter_api::ProcessAdapter;
use omni_core::spec::adapter::AdapterManifest;
use std::time::Duration;

const ADAPTER_MANIFEST_SRC: &str = include_str!("../../../../adapters/node-backend/adapter.yaml");
const ADAPTER_SCRIPT_SRC: &str = include_str!("../../../../adapters/node-backend/adapter.js");
const SCRIPT_FILENAME: &str = "adapter.js";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Parses and schema-validates the embedded manifest. Panics on failure —
/// that would mean this binary shipped with a broken manifest, which is a
/// build-time defect, not a runtime condition any caller should handle.
pub fn manifest() -> AdapterManifest {
    let yaml: serde_yaml::Value = serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded node-backend adapter.yaml is valid YAML");
    let json: serde_json::Value =
        serde_json::to_value(&yaml).expect("yaml->json for node-backend manifest");
    let errors = omni_core::schema::validate(
        omni_core::schema::SchemaKind::Adapter,
        &json,
        "adapters/node-backend/adapter.yaml",
    );
    assert!(
        errors.is_empty(),
        "embedded node-backend adapter manifest failed schema validation: {errors:?}"
    );
    serde_yaml::from_str(ADAPTER_MANIFEST_SRC)
        .expect("embedded node-backend adapter.yaml parses into AdapterManifest")
}

pub fn build() -> ProcessAdapter {
    build_with_candidates(omni_adapter_api::known_aliases("node"))
}

/// Exposed so tests can force toolchain-resolution failure deterministically
/// (a bogus candidate list) without relying on the test machine actually
/// lacking Node, which would be environment-dependent and flaky.
pub fn build_with_candidates(launcher_candidates: Vec<String>) -> ProcessAdapter {
    ProcessAdapter::new(
        manifest(),
        launcher_candidates,
        SCRIPT_FILENAME,
        ADAPTER_SCRIPT_SRC,
        DEFAULT_TIMEOUT,
    )
}

/// The embedded script source and its materialized filename, exposed for
/// tests that need to drive the real adapter process directly (bypassing
/// `ProcessAdapter`) to exercise protocol edge cases like an unsupported
/// operation or a hand-crafted unsafe path.
pub fn script_source() -> &'static str {
    ADAPTER_SCRIPT_SRC
}

pub fn script_filename() -> &'static str {
    SCRIPT_FILENAME
}
