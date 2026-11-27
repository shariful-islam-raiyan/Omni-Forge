//! `omni adapter validate <path>`: validates a third-party (or in-progress)
//! adapter manifest **on its own**, with no `Adapter` trait implementation
//! attached and no registration into this binary — exactly the tool a
//! developer writing a new adapter (per `docs/adapter-protocol.md`'s
//! extension guide) needs before wiring their manifest into
//! `crates/omni-cli/src/registry.rs`.
//!
//! Two checks, same ones the embedded manifests already pass at compile
//! time (`crates/omni-cli/src/adapters/*.rs`'s `manifest()` functions):
//! schema validation (`specs/adapter.schema.json`) and the conformance
//! invariants (`omni_adapter_api::conformance::check_manifest`) every
//! shipped adapter's own test suite already calls. This command does not
//! register, load, or execute the adapter in any way -- see
//! `docs/security-model.md`: no third-party adapter is trusted or run by
//! this binary, and this command does not change that.

use omni_core::spec::adapter::AdapterManifest;
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_manifest_path(input: &Path) -> Result<PathBuf, String> {
    if input.is_dir() {
        let candidate = input.join("adapter.yaml");
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(format!(
            "'{}' is a directory but contains no 'adapter.yaml'",
            input.display()
        ));
    }
    if input.is_file() {
        return Ok(input.to_path_buf());
    }
    Err(format!("'{}' does not exist", input.display()))
}

pub fn run(path: &Path, json: bool) -> i32 {
    let manifest_path = match resolve_manifest_path(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: E-ADAPTER-VALIDATE-NOT-FOUND: {e}");
            return 1;
        }
    };

    let raw = match fs::read_to_string(&manifest_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "error: E-ADAPTER-VALIDATE-IO: could not read '{}': {e}",
                manifest_path.display()
            );
            return 1;
        }
    };

    let yaml_value: serde_yaml::Value = match serde_yaml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "error: E-ADAPTER-VALIDATE-YAML: '{}' is not valid YAML: {e}",
                manifest_path.display()
            );
            return 1;
        }
    };
    let json_value: serde_json::Value = serde_json::to_value(&yaml_value).unwrap_or_default();

    let schema_errors = omni_core::schema::validate(
        omni_core::schema::SchemaKind::Adapter,
        &json_value,
        &manifest_path.display().to_string(),
    );
    if !schema_errors.is_empty() {
        if json {
            print_json(
                &manifest_path,
                false,
                &[],
                &schema_errors_as_strings(&schema_errors),
            );
        } else {
            println!("Validating '{}'\n", manifest_path.display());
            println!(
                "FAILED schema validation ({} error(s)):",
                schema_errors.len()
            );
            for d in &schema_errors {
                println!("  - {}: {}", d.code, d.message);
            }
        }
        return 1;
    }

    let manifest: AdapterManifest = match serde_yaml::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "error: E-ADAPTER-VALIDATE-PARSE: '{}' passed schema validation but failed to parse into an adapter manifest: {e}",
                manifest_path.display()
            );
            return 1;
        }
    };

    let violations = omni_adapter_api::conformance::check_manifest(&manifest);

    if json {
        print_json(&manifest_path, violations.is_empty(), &violations, &[]);
    } else {
        println!("Validating '{}'\n", manifest_path.display());
        println!("  id: {}", manifest.id);
        println!("  version: {}", manifest.version);
        println!("  protocol: {}", manifest.protocol);
        println!("  targets: {}", manifest.capabilities.targets.join(", "));
        println!(
            "  operations: {}",
            manifest.capabilities.operations.join(", ")
        );
        println!();
        if violations.is_empty() {
            println!("PASSED: schema-valid and conformant.");
        } else {
            println!("FAILED conformance ({} violation(s)):", violations.len());
            for v in &violations {
                println!("  - {v}");
            }
        }
    }

    if violations.is_empty() {
        0
    } else {
        1
    }
}

fn schema_errors_as_strings(errors: &[omni_core::diagnostics::Diagnostic]) -> Vec<String> {
    errors
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn print_json(
    path: &Path,
    passed: bool,
    conformance_violations: &[String],
    schema_errors: &[String],
) {
    let payload = serde_json::json!({
        "path": path.display().to_string(),
        "passed": passed,
        "schema_errors": schema_errors,
        "conformance_violations": conformance_violations,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );
}
