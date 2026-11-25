//! Exercises `omni-core`'s loader against the static fixtures in
//! `fixtures/`, so those files are a real, checked part of the test suite
//! rather than documentation that can silently drift.

use omni_core::loader::load_project;
use std::path::PathBuf;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

#[test]
fn valid_minimal_web_project_loads_cleanly() {
    let (_loaded, diags) = load_project(&fixture("valid-projects/minimal-web")).unwrap();
    assert!(!diags.has_errors());
}

#[test]
fn missing_adapter_fixture_fails_with_expected_code() {
    let err = load_project(&fixture("invalid-projects/missing-adapter")).unwrap_err();
    match err {
        omni_core::OmniError::Invalid(items) => {
            assert!(items.iter().any(|d| d.code == "E-PROJECT-MISSING-ADAPTER"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn unknown_target_fixture_fails_with_expected_code() {
    let err = load_project(&fixture("invalid-projects/unknown-target")).unwrap_err();
    match err {
        omni_core::OmniError::Invalid(items) => {
            assert!(items.iter().any(|d| d.code == "E-PROJECT-UNKNOWN-TARGET"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn bad_schema_version_fixture_is_rejected() {
    let result = load_project(&fixture("invalid-projects/bad-schema-version"));
    assert!(result.is_err());
}
