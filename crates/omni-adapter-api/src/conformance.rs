//! A reusable conformance check any adapter implementation can run against
//! itself in its own test suite, so a future second/third adapter has a
//! concrete bar to pass rather than an informal description.

use crate::adapter::Adapter;
use omni_core::spec::adapter::AdapterManifest;

const KNOWN_OPERATIONS: &[&str] = &[
    "discover", "validate", "resolve", "compile", "bundle", "package", "test", "run", "debug",
    "inspect",
];
const KNOWN_KINDS: &[&str] = &["ui", "logic", "service", "library", "tool"];
const KNOWN_PROTOCOLS: &[&str] = &["in-process/v1", "out-of-process/v1"];

/// Returns a list of violations; an adapter is conformant iff this is empty.
/// Takes a live `&dyn Adapter` -- for checking a manifest on its own
/// (e.g. one read from disk, with no `Adapter` implementation attached,
/// as `omni adapter validate` does), use [`check_manifest`] directly.
pub fn check_manifest_invariants(adapter: &dyn Adapter) -> Vec<String> {
    check_manifest(adapter.manifest())
}

/// The same checks [`check_manifest_invariants`] runs, against a manifest
/// value directly rather than a live adapter instance.
pub fn check_manifest(m: &AdapterManifest) -> Vec<String> {
    let mut violations = Vec::new();

    if m.id.is_empty()
        || !m
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        violations.push(format!(
            "adapter id '{}' must be non-empty lowercase-kebab-case",
            m.id
        ));
    }
    if m.version.split('.').count() < 3 {
        violations.push(format!(
            "adapter version '{}' must look like semver (x.y.z)",
            m.version
        ));
    }
    if !KNOWN_PROTOCOLS.contains(&m.protocol.as_str()) {
        violations.push(format!("unknown protocol '{}'", m.protocol));
    }
    if m.capabilities.targets.is_empty() {
        violations.push("capabilities.targets must declare at least one target".into());
    }
    if m.capabilities.operations.is_empty() {
        violations.push("capabilities.operations must declare at least one operation".into());
    }
    for op in &m.capabilities.operations {
        if !KNOWN_OPERATIONS.contains(&op.as_str()) {
            violations.push(format!("unknown operation '{op}' declared"));
        }
    }
    for kind in &m.capabilities.component_kinds {
        if !KNOWN_KINDS.contains(&kind.as_str()) {
            violations.push(format!("unknown component kind '{kind}' declared"));
        }
    }
    if m.capabilities.component_kinds.is_empty() {
        violations.push("capabilities.component_kinds must declare at least one kind".into());
    }

    violations
}
