//! JSON Schema validation for the v1 specification documents.
//!
//! Schemas are embedded at compile time from `specs/*.schema.json` so the
//! CLI binary is self-contained and validation can't drift from a file that
//! didn't ship with the binary.

use crate::diagnostics::Diagnostic;
use jsonschema::JSONSchema;
use serde_json::Value;
use std::sync::OnceLock as StdOnceLock;

const PROJECT_SCHEMA_SRC: &str = include_str!("../../../specs/project.schema.json");
const COMPONENT_SCHEMA_SRC: &str = include_str!("../../../specs/component.schema.json");
const ADAPTER_SCHEMA_SRC: &str = include_str!("../../../specs/adapter.schema.json");
const WEBAPP_SCHEMA_SRC: &str = include_str!("../../../specs/webapp.schema.json");
const BUILD_REPORT_SCHEMA_SRC: &str = include_str!("../../../specs/build-report.schema.json");
const API_CONTRACT_SCHEMA_SRC: &str = include_str!("../../../specs/api-contract.schema.json");
const ANDROID_APP_SCHEMA_SRC: &str = include_str!("../../../specs/android-app.schema.json");

pub enum SchemaKind {
    Project,
    Component,
    Adapter,
    WebApp,
    BuildReport,
    ApiContract,
    AndroidApp,
}

fn compiled(src: &'static str) -> &'static JSONSchema {
    fn compile_one(
        cell: &'static StdOnceLock<JSONSchema>,
        src: &'static str,
    ) -> &'static JSONSchema {
        cell.get_or_init(|| {
            let value: Value = serde_json::from_str(src).expect("embedded schema is valid JSON");
            JSONSchema::compile(&value).expect("embedded schema is valid JSON Schema")
        })
    }

    static PROJECT: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static COMPONENT: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static ADAPTER: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static WEBAPP: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static BUILD_REPORT: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static API_CONTRACT: StdOnceLock<JSONSchema> = StdOnceLock::new();
    static ANDROID_APP: StdOnceLock<JSONSchema> = StdOnceLock::new();

    if std::ptr::eq(src, PROJECT_SCHEMA_SRC) {
        compile_one(&PROJECT, src)
    } else if std::ptr::eq(src, COMPONENT_SCHEMA_SRC) {
        compile_one(&COMPONENT, src)
    } else if std::ptr::eq(src, ADAPTER_SCHEMA_SRC) {
        compile_one(&ADAPTER, src)
    } else if std::ptr::eq(src, WEBAPP_SCHEMA_SRC) {
        compile_one(&WEBAPP, src)
    } else if std::ptr::eq(src, API_CONTRACT_SCHEMA_SRC) {
        compile_one(&API_CONTRACT, src)
    } else if std::ptr::eq(src, ANDROID_APP_SCHEMA_SRC) {
        compile_one(&ANDROID_APP, src)
    } else {
        compile_one(&BUILD_REPORT, src)
    }
}

impl SchemaKind {
    fn source(&self) -> &'static str {
        match self {
            SchemaKind::Project => PROJECT_SCHEMA_SRC,
            SchemaKind::Component => COMPONENT_SCHEMA_SRC,
            SchemaKind::Adapter => ADAPTER_SCHEMA_SRC,
            SchemaKind::WebApp => WEBAPP_SCHEMA_SRC,
            SchemaKind::BuildReport => BUILD_REPORT_SCHEMA_SRC,
            SchemaKind::ApiContract => API_CONTRACT_SCHEMA_SRC,
            SchemaKind::AndroidApp => ANDROID_APP_SCHEMA_SRC,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            SchemaKind::Project => "E-SCHEMA-PROJECT",
            SchemaKind::Component => "E-SCHEMA-COMPONENT",
            SchemaKind::Adapter => "E-SCHEMA-ADAPTER",
            SchemaKind::WebApp => "E-SCHEMA-WEBAPP",
            SchemaKind::BuildReport => "E-SCHEMA-BUILD-REPORT",
            SchemaKind::ApiContract => "E-SCHEMA-API-CONTRACT",
            SchemaKind::AndroidApp => "E-SCHEMA-ANDROID-APP",
        }
    }
}

/// Validates `value` against the named schema, returning one [`Diagnostic`]
/// per violation (empty vec means valid).
pub fn validate(kind: SchemaKind, value: &Value, file: &str) -> Vec<Diagnostic> {
    let schema = compiled(kind.source());
    let code = kind.error_code();
    match schema.validate(value) {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .map(|e| {
                let pointer = e.instance_path.to_string();
                Diagnostic::error(code, format!("{e}"))
                    .with_file(file)
                    .with_explanation(format!("at JSON pointer '{pointer}'"))
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_unknown_schema_version() {
        let value = json!({
            "schema": "omniforge.dev/project/v2",
            "project": {"name": "x", "version": "0.1.0"},
            "targets": {"web": {"enabled": true}}
        });
        let diags = validate(SchemaKind::Project, &value, "omni.yaml");
        assert!(!diags.is_empty());
    }

    #[test]
    fn accepts_minimal_valid_project() {
        let value = json!({
            "schema": "omniforge.dev/project/v1",
            "project": {"name": "hello-web", "version": "0.1.0"},
            "targets": {"web": {"enabled": true, "adapter": "web-static", "entry": "web/app.yaml"}}
        });
        let diags = validate(SchemaKind::Project, &value, "omni.yaml");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn accepts_valid_out_of_process_adapter_manifest() {
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "python-backend",
            "version": "0.1.0",
            "protocol": "out-of-process/v1",
            "capabilities": {
                "languages": ["python"],
                "component_kinds": ["service"],
                "targets": ["backend"],
                "operations": ["validate", "compile"],
                "requires": ["python3"]
            }
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn rejects_adapter_manifest_missing_capabilities() {
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "broken-adapter",
            "version": "0.1.0",
            "protocol": "out-of-process/v1"
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(!diags.is_empty());
    }

    #[test]
    fn rejects_adapter_manifest_with_retired_protocol_tag() {
        // "jsonrpc/v1" was the original placeholder tag for out-of-process
        // adapters before any existed; it was renamed to "out-of-process/v1"
        // once a real one (python-backend) was built. Old manifests using
        // the retired tag must fail validation, not be silently accepted.
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "old-style-adapter",
            "version": "0.1.0",
            "protocol": "jsonrpc/v1",
            "capabilities": {
                "languages": [],
                "component_kinds": ["service"],
                "targets": ["backend"],
                "operations": ["compile"]
            }
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(!diags.is_empty());
    }

    #[test]
    fn accepts_adapter_manifest_with_a_dev_block() {
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "python-backend",
            "version": "0.1.0",
            "protocol": "out-of-process/v1",
            "capabilities": {
                "languages": ["python"],
                "component_kinds": ["service"],
                "targets": ["backend"],
                "operations": ["validate", "compile"],
                "requires": ["python3"]
            },
            "dev": {
                "args": ["--serve", "{port}"],
                "ready_stdout_prefix": "LISTENING",
                "ready_timeout_ms": 10000,
                "stop_grace_ms": 3000
            }
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn rejects_adapter_manifest_dev_block_missing_required_field() {
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "python-backend",
            "version": "0.1.0",
            "protocol": "out-of-process/v1",
            "capabilities": {
                "languages": ["python"],
                "component_kinds": ["service"],
                "targets": ["backend"],
                "operations": ["validate", "compile"],
                "requires": ["python3"]
            },
            "dev": {
                "args": ["--serve", "{port}"]
            }
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(!diags.is_empty());
    }

    #[test]
    fn rejects_adapter_manifest_with_unknown_operation() {
        let value = json!({
            "schema": "omniforge.dev/adapter/v1",
            "id": "weird-adapter",
            "version": "0.1.0",
            "protocol": "in-process/v1",
            "capabilities": {
                "languages": [],
                "component_kinds": ["ui"],
                "targets": ["web"],
                "operations": ["teleport"]
            }
        });
        let diags = validate(SchemaKind::Adapter, &value, "adapter.yaml");
        assert!(!diags.is_empty());
    }

    #[test]
    fn accepts_minimal_android_app_manifest() {
        let value = json!({
            "schema": "omniforge.dev/android-app/v1",
            "package": "dev.omniforge.hello",
            "app_label": "Hello OmniForge",
            "root": {"component": "ui.home"}
        });
        let diags = validate(SchemaKind::AndroidApp, &value, "android/app.yaml");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn rejects_android_app_manifest_with_invalid_package_name() {
        let value = json!({
            "schema": "omniforge.dev/android-app/v1",
            "package": "NotAValidPackage",
            "app_label": "Hello OmniForge",
            "root": {"component": "ui.home"}
        });
        let diags = validate(SchemaKind::AndroidApp, &value, "android/app.yaml");
        assert!(!diags.is_empty());
    }
}
