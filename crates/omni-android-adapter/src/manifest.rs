use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ANDROID_APP_SCHEMA_VERSION: &str = "omniforge.dev/android-app/v1";

pub const DEFAULT_MIN_SDK: u32 = 24;
pub const DEFAULT_TARGET_SDK: u32 = 36;
pub const DEFAULT_COMPILE_SDK: u32 = 36;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidAppManifest {
    pub schema: String,
    pub package: String,
    pub app_label: String,
    #[serde(default)]
    pub min_sdk: Option<u32>,
    #[serde(default)]
    pub target_sdk: Option<u32>,
    #[serde(default)]
    pub compile_sdk: Option<u32>,
    pub root: RootRef,
}

impl AndroidAppManifest {
    pub fn min_sdk(&self) -> u32 {
        self.min_sdk.unwrap_or(DEFAULT_MIN_SDK)
    }
    pub fn target_sdk(&self) -> u32 {
        self.target_sdk.unwrap_or(DEFAULT_TARGET_SDK)
    }
    pub fn compile_sdk(&self) -> u32 {
        self.compile_sdk.unwrap_or(DEFAULT_COMPILE_SDK)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootRef {
    pub component: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
}
