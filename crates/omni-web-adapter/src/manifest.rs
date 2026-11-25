use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WEBAPP_SCHEMA_VERSION: &str = "omniforge.dev/webapp/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebAppManifest {
    pub schema: String,
    pub title: String,
    pub root: RootRef,
    #[serde(default)]
    pub public: PublicAssets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootRef {
    pub component: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PublicAssets {
    #[serde(default)]
    pub css: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
    #[serde(default)]
    pub assets: Vec<String>,
}
