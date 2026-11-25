use crate::diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};

pub const BUILD_REPORT_SCHEMA_VERSION: &str = "omniforge.dev/build-report/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetStatus {
    Success,
    Failed,
    Partial,
    Unsupported,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverallStatus {
    Success,
    Failed,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheResult {
    Hit,
    Miss,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetReport {
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<AdapterRef>,
    pub status: TargetStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheResult>,
    pub artifacts: Vec<ArtifactRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRef {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    pub schema: String,
    pub project: ProjectRef,
    pub timestamp: String,
    pub status: OverallStatus,
    pub targets: Vec<TargetReport>,
}

impl BuildReport {
    pub fn new(project: ProjectRef, targets: Vec<TargetReport>) -> Self {
        let any_failed = targets.iter().any(|t| t.status == TargetStatus::Failed);
        let any_partial = targets
            .iter()
            .any(|t| matches!(t.status, TargetStatus::Partial | TargetStatus::Unsupported));
        let all_ok = targets.iter().all(|t| t.status == TargetStatus::Success);

        let status = if any_failed {
            OverallStatus::Failed
        } else if all_ok {
            OverallStatus::Success
        } else if any_partial {
            OverallStatus::Partial
        } else {
            OverallStatus::Success
        };

        Self {
            schema: BUILD_REPORT_SCHEMA_VERSION.to_string(),
            project,
            timestamp: chrono::Utc::now().to_rfc3339(),
            status,
            targets,
        }
    }
}
