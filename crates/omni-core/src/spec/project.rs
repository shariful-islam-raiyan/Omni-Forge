use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROJECT_SCHEMA_VERSION: &str = "omniforge.dev/project/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    pub schema: String,
    pub project: ProjectInfo,
    pub targets: BTreeMap<String, TargetConfig>,
    #[serde(default)]
    pub components: ComponentsConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub enabled: bool,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub entry: Option<String>,
    /// Path (relative to the project root) to an API contract document
    /// (`specs/api-contract.schema.json`) this target's backend implements.
    /// When set, `omni build`/`omni check` validate the contract as part
    /// of building this target and its bytes become part of the build
    /// cache key — a real dependency edge from this target to the
    /// contract, not just documentation. Additive, optional; introduced in
    /// the "Unified Application Runtime" milestone (see
    /// `docs/project-format.md`).
    #[serde(default)]
    pub contract: Option<String>,
    /// Development-mode settings for `omni dev`. Additive, optional;
    /// introduced alongside `contract` above.
    #[serde(default)]
    pub dev: Option<TargetDevConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TargetDevConfig {
    /// A fixed port to use for this target's dev server. When absent,
    /// `omni dev` auto-picks a free ephemeral port instead — the default,
    /// conflict-free behavior; set this only when a stable, predictable
    /// port is actually wanted.
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentsConfig {
    #[serde(default = "default_components_dir")]
    pub directory: String,
}

impl Default for ComponentsConfig {
    fn default() -> Self {
        Self {
            directory: default_components_dir(),
        }
    }
}

fn default_components_dir() -> String {
    "lib/components".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default = "default_output_dir")]
    pub output: String,
    #[serde(default = "default_true")]
    pub cache: bool,
    #[serde(default)]
    pub parallel: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            output: default_output_dir(),
            cache: true,
            parallel: false,
        }
    }
}

fn default_output_dir() -> String {
    "dist".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub strict_adapter_permissions: bool,
    #[serde(default = "default_true")]
    pub audit_dependencies: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            strict_adapter_permissions: true,
            audit_dependencies: true,
        }
    }
}

/// The set of target names OmniForge's core vocabulary recognizes, even if
/// no adapter currently implements them. Enabling a recognized-but-
/// unimplemented target is a validation error, not a silent no-op.
pub const KNOWN_TARGETS: &[&str] = &[
    "web", "android", "ios", "windows", "linux", "macos", "backend",
];
