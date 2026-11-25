//! The versioned adapter protocol message shapes.
//!
//! These are the real wire types: `crate::process::ProcessAdapter` serializes
//! an [`AdapterRequest`] as one line of JSON to a child process's stdin and
//! parses one line of JSON from its stdout as an [`AdapterResponse`] — see
//! `crate::process` for the execution engine and `docs/adapter-protocol.md`
//! for the full protocol description, including the trust/enablement policy
//! for out-of-process adapters (today: only adapters this repository ships
//! and reviews, e.g. `python-backend`; no third-party/dynamically-discovered
//! adapter is ever spawned).

use omni_core::diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL_VERSION: &str = "omniforge.dev/adapter-protocol/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Discover,
    Validate,
    Resolve,
    Compile,
    Bundle,
    Package,
    Test,
    Run,
    Debug,
    Inspect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRequest {
    pub protocol: String,
    pub request_id: String,
    pub operation: Operation,
    pub project_root: String,
    pub target: String,
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub source_files: Vec<String>,
    /// The target's `entry` file (e.g. `backend/service.json`), already
    /// resolved to an absolute path by the core, if the target config
    /// declared one. The adapter decides how to parse its own contents —
    /// the core never does.
    #[serde(default)]
    pub entry_path: Option<String>,
    pub output_dir: String,
    #[serde(default)]
    pub config: BTreeMap<String, serde_json::Value>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResponse {
    pub protocol: String,
    pub request_id: String,
    pub status: ResponseStatus,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

impl AdapterRequest {
    pub fn is_compatible_protocol(&self) -> bool {
        self.protocol == PROTOCOL_VERSION
    }
}
