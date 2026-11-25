use serde::{Deserialize, Serialize};

pub const ADAPTER_SCHEMA_VERSION: &str = "omniforge.dev/adapter/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub schema: String,
    pub id: String,
    pub version: String,
    pub protocol: String,
    pub capabilities: AdapterCapabilities,
    /// Optional dev-server-serving capability: how to run this adapter's
    /// build *output* as a long-running process for `omni dev` (distinct
    /// from `compile`, which is bounded and run-to-completion). Absent for
    /// adapters that don't support live dev serving (e.g. `web-static`,
    /// whose "dev serving" is just the CLI's own static file server).
    #[serde(default)]
    pub dev: Option<DevServeConfig>,
}

/// Declares how an out-of-process adapter's packaged build output can be
/// started as a long-running dev server. The entry script and config file
/// are discovered at `omni dev` time from the target's actual build
/// artifacts (see `docs/adapter-protocol.md`'s "runnable-package
/// convention"); this struct only covers the parts that are the same for
/// every build of this adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevServeConfig {
    /// Extra arguments appended after `<entry script> <config file>`.
    /// `{port}` is substituted with the resolved dev port.
    pub args: Vec<String>,
    /// The process is considered ready once it prints one stdout line
    /// whose trimmed start matches this prefix (e.g. `"LISTENING"`).
    pub ready_stdout_prefix: String,
    pub ready_timeout_ms: u64,
    /// How long to wait for a graceful stop before a hard kill.
    pub stop_grace_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub languages: Vec<String>,
    pub component_kinds: Vec<String>,
    pub targets: Vec<String>,
    pub operations: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
}

impl AdapterCapabilities {
    pub fn supports_target(&self, target: &str) -> bool {
        self.targets.iter().any(|t| t == target)
    }

    pub fn supports_kind(&self, kind: &str) -> bool {
        self.component_kinds.iter().any(|k| k == kind)
    }

    pub fn supports_operation(&self, op: &str) -> bool {
        self.operations.iter().any(|o| o == op)
    }
}
