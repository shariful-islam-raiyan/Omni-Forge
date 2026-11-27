use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WINDOWS_APP_SCHEMA_VERSION: &str = "omniforge.dev/windows-app/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsAppManifest {
    pub schema: String,
    pub app_name: String,
    pub root: RootRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootRef {
    pub component: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
}
