//! The OmniForge API contract format: a small, custom document for
//! describing HTTP methods, paths, request/response bodies (each an inline
//! JSON Schema draft-07 fragment, the same dialect every other
//! `specs/*.schema.json` file already uses), and per-status-code responses
//! including error shapes. See `docs/api-contract-format.md` for the full
//! write-up and a worked example.
//!
//! **This is not OpenAPI, and is not claimed to be.** It borrows familiar
//! vocabulary (`paths`, an HTTP-method-keyed operation, `responses`) but is
//! not a valid OpenAPI document by any real OpenAPI tool's reading: there
//! is no `openapi` version field (this format's own `schema` field plays
//! that role), request/response bodies are a flat `body` key rather than
//! `content.<mediaType>.schema`, and the body dialect is JSON Schema
//! draft-07 rather than OpenAPI 3.1's 2020-12-based `base` dialect. An
//! earlier version of this file (and its docs) described this as "a subset
//! of OpenAPI 3.1," which was inaccurate — see
//! `docs/decisions/0008-api-contract-is-a-custom-format.md` for the
//! correction and exactly what was wrong with the original claim.
//!
//! Deliberately small regardless of the OpenAPI question: no `$ref`, no
//! `components`, no `servers` list, only `get`/`post` methods — see
//! `docs/decisions/0006-api-contract-format.md` for the original scoping
//! rationale (still accurate for the *size* of the format, just not for the
//! OpenAPI-compatibility claim).
//!
//! This module parses and schema-validates the document itself, and
//! ([`ApiOperation::validate_request_body`]/[`ApiResponse::validate_body`])
//! validates an actual JSON payload against one of its declared body
//! fragments with the `jsonschema` crate. It does not call out to a real
//! HTTP server itself — doing that with a real response body is the
//! caller's job (today: `crates/omni-cli/tests/fullstack_http.rs`).

use crate::diagnostics::Diagnostic;
use crate::schema::{self, SchemaKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const API_CONTRACT_SCHEMA_VERSION: &str = "omniforge.dev/api-contract/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiRequest {
    /// An inline JSON Schema draft-07 fragment the request body must match.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub description: String,
    /// An inline JSON Schema draft-07 fragment the response body must match.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiOperation {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub request: Option<ApiRequest>,
    /// Keyed by HTTP status code as a string, e.g. `"200"`, `"404"`.
    pub responses: BTreeMap<String, ApiResponse>,
}

/// Compiles `fragment` as a JSON Schema (draft-07) and validates `value`
/// against it, returning every violation message (empty means valid).
fn validate_fragment(fragment: &serde_json::Value, value: &serde_json::Value) -> Vec<String> {
    match jsonschema::JSONSchema::compile(fragment) {
        Ok(compiled) => match compiled.validate(value) {
            Ok(()) => Vec::new(),
            Err(errors) => errors.map(|e| e.to_string()).collect(),
        },
        Err(e) => vec![format!("declared body fragment is not a valid schema: {e}")],
    }
}

impl ApiOperation {
    /// Validates `body` against this operation's declared request body
    /// schema. `Ok(())` if there is no declared request body at all (an
    /// operation with no `request.body` imposes no constraint) or if
    /// `body` matches it; `Err(messages)` with every violation otherwise.
    pub fn validate_request_body(&self, body: &serde_json::Value) -> Result<(), Vec<String>> {
        let Some(fragment) = self.request.as_ref().and_then(|r| r.body.as_ref()) else {
            return Ok(());
        };
        let errors = validate_fragment(fragment, body);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl ApiResponse {
    /// Validates `body` against this response's declared body schema.
    /// `Ok(())` if there is no declared body schema or `body` matches it;
    /// `Err(messages)` with every violation otherwise.
    pub fn validate_body(&self, body: &serde_json::Value) -> Result<(), Vec<String>> {
        let Some(fragment) = self.body.as_ref() else {
            return Ok(());
        };
        let errors = validate_fragment(fragment, body);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiContract {
    pub schema: String,
    pub info: ApiInfo,
    /// Keyed by path (e.g. `"/api/greeting"`), then by lowercase HTTP method
    /// (`"get"`/`"post"`).
    pub paths: BTreeMap<String, BTreeMap<String, ApiOperation>>,
}

impl ApiContract {
    /// Looks up the declared operation for `path`/`method` (method matched
    /// case-insensitively), if the contract declares one.
    pub fn operation(&self, path: &str, method: &str) -> Option<&ApiOperation> {
        self.paths.get(path)?.get(&method.to_lowercase())
    }
}

/// Parses and schema-validates one API contract document from its raw JSON
/// text. Returns the parsed contract, or every schema violation found (never
/// a partial/best-effort result).
pub fn parse(raw: &str, file: &str) -> Result<ApiContract, Vec<Diagnostic>> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        vec![Diagnostic::error(
            "E-API-CONTRACT-JSON",
            format!("could not parse '{file}' as JSON: {e}"),
        )
        .with_file(file)]
    })?;

    let errors = schema::validate(SchemaKind::ApiContract, &value, file);
    if !errors.is_empty() {
        return Err(errors);
    }

    serde_json::from_value(value).map_err(|e| {
        vec![Diagnostic::error(
            "E-API-CONTRACT-PARSE",
            format!("'{file}' passed schema validation but failed to deserialize: {e}"),
        )
        .with_file(file)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
        "schema": "omniforge.dev/api-contract/v1",
        "info": { "title": "hello-fullstack API", "version": "1.0.0" },
        "paths": {
            "/api/greeting": {
                "get": {
                    "summary": "Return a greeting",
                    "responses": {
                        "200": {
                            "description": "A greeting was produced",
                            "body": {
                                "type": "object",
                                "required": ["greeting", "count"],
                                "properties": {
                                    "greeting": { "type": "string" },
                                    "count": { "type": "integer" }
                                }
                            }
                        }
                    }
                }
            },
            "/api/echo": {
                "post": {
                    "summary": "Echo a message back",
                    "request": {
                        "body": {
                            "type": "object",
                            "required": ["message"],
                            "properties": { "message": { "type": "string" } }
                        }
                    },
                    "responses": {
                        "200": { "description": "Echoed", "body": { "type": "object" } },
                        "400": { "description": "Invalid request body", "body": { "type": "object" } }
                    }
                }
            }
        }
    }"#;

    #[test]
    fn parses_a_valid_contract() {
        let contract = parse(VALID, "api-contract.json").expect("should parse");
        assert_eq!(contract.schema, API_CONTRACT_SCHEMA_VERSION);
        let op = contract
            .operation("/api/greeting", "GET")
            .expect("declared operation");
        assert!(op.responses.contains_key("200"));
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let raw = VALID.replace(
            "omniforge.dev/api-contract/v1",
            "omniforge.dev/api-contract/v2",
        );
        let err = parse(&raw, "api-contract.json").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn rejects_operation_missing_responses() {
        let raw = r#"{
            "schema": "omniforge.dev/api-contract/v1",
            "info": { "title": "x", "version": "1.0.0" },
            "paths": { "/api/x": { "get": { "summary": "no responses" } } }
        }"#;
        let err = parse(raw, "api-contract.json").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn validate_request_body_accepts_a_matching_payload() {
        let contract = parse(VALID, "api-contract.json").expect("should parse");
        let op = contract.operation("/api/echo", "POST").unwrap();
        let body = serde_json::json!({ "message": "hi" });
        assert!(op.validate_request_body(&body).is_ok());
    }

    #[test]
    fn validate_request_body_rejects_a_missing_required_field() {
        let contract = parse(VALID, "api-contract.json").expect("should parse");
        let op = contract.operation("/api/echo", "POST").unwrap();
        let body = serde_json::json!({});
        let errors = op.validate_request_body(&body).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn validate_body_rejects_a_wrong_type() {
        let contract = parse(VALID, "api-contract.json").expect("should parse");
        let op = contract.operation("/api/greeting", "GET").unwrap();
        let response = op.responses.get("200").unwrap();
        // "greeting" must be a string per the declared schema.
        let body = serde_json::json!({ "greeting": 12345, "count": 1 });
        let errors = response.validate_body(&body).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn validate_body_accepts_a_matching_response() {
        let contract = parse(VALID, "api-contract.json").expect("should parse");
        let op = contract.operation("/api/greeting", "GET").unwrap();
        let response = op.responses.get("200").unwrap();
        let body = serde_json::json!({ "greeting": "hi", "count": 1 });
        assert!(response.validate_body(&body).is_ok());
    }

    #[test]
    fn rejects_unknown_http_method() {
        let raw = r#"{
            "schema": "omniforge.dev/api-contract/v1",
            "info": { "title": "x", "version": "1.0.0" },
            "paths": { "/api/x": { "delete": { "responses": { "200": { "description": "ok" } } } } }
        }"#;
        let err = parse(raw, "api-contract.json").unwrap_err();
        assert!(!err.is_empty());
    }
}
