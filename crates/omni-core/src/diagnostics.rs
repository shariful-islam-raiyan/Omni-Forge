//! Structured diagnostics with stable error codes.
//!
//! Every user-facing failure in OmniForge should be representable as a
//! [`Diagnostic`] rather than an opaque string, so CLI output, build reports,
//! and (eventually) IDE integrations can rely on stable `code` values.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.into(),
            explanation: None,
            file: None,
            line: None,
            component: None,
            target: None,
            suggestion: None,
        }
    }

    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Warning,
            message: message.into(),
            explanation: None,
            file: None,
            line: None,
            component: None,
            target: None,
            suggestion: None,
        }
    }

    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Info,
            message: message.into(),
            explanation: None,
            file: None,
            line: None,
            component: None,
            target: None,
            suggestion: None,
        }
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = Some(explanation.into());
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.severity, self.code, self.message)?;
        if let Some(file) = &self.file {
            write!(f, " ({file}")?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, ")")?;
        }
        if let Some(suggestion) = &self.suggestion {
            write!(f, "\n  suggestion: {suggestion}")?;
        }
        Ok(())
    }
}

/// Collects diagnostics during a pass (config loading, resolution, build).
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(Diagnostic::is_error)
    }

    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    pub fn into_items(self) -> Vec<Diagnostic> {
        self.items
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An out-of-process adapter (e.g. the Python one) sends diagnostics as
    /// plain JSON and will naturally omit optional keys it has no value
    /// for, rather than writing them out as explicit `null`s. Every
    /// `Option<T>` field here must deserialize from a *missing* key, not
    /// just a present-but-null one.
    #[test]
    fn deserializes_from_json_that_omits_optional_fields() {
        let json = r#"{"code": "E-TEST", "severity": "error", "message": "boom"}"#;
        let d: Diagnostic = serde_json::from_str(json).unwrap();
        assert_eq!(d.code, "E-TEST");
        assert!(d.explanation.is_none());
        assert!(d.file.is_none());
        assert!(d.line.is_none());
        assert!(d.component.is_none());
        assert!(d.target.is_none());
        assert!(d.suggestion.is_none());
    }

    #[test]
    fn severity_round_trips_through_lowercase_json() {
        let d: Diagnostic =
            serde_json::from_str(r#"{"code":"X","severity":"warning","message":"m"}"#).unwrap();
        assert_eq!(d.severity, Severity::Warning);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"severity\":\"warning\""));
    }
}
