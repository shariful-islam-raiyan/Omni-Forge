use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const COMPONENT_SCHEMA_VERSION: &str = "omniforge.dev/component/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentKind {
    Ui,
    Logic,
    Service,
    Library,
    Tool,
}

impl std::fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ComponentKind::Ui => "ui",
            ComponentKind::Logic => "logic",
            ComponentKind::Service => "service",
            ComponentKind::Library => "library",
            ComponentKind::Tool => "tool",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentManifest {
    pub schema: String,
    pub name: String,
    pub version: String,
    pub kind: ComponentKind,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub interface: ComponentInterface,
    pub implementations: BTreeMap<String, ComponentImplementation>,

    /// Populated by the discovery pass, not part of the on-disk schema.
    #[serde(skip)]
    pub manifest_path: std::path::PathBuf,
    #[serde(skip)]
    pub manifest_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentInterface {
    #[serde(default)]
    pub inputs: BTreeMap<String, FieldSpec>,
    #[serde(default)]
    pub outputs: BTreeMap<String, FieldSpec>,
    #[serde(default)]
    pub events: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Boolean,
    Number,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentImplementation {
    pub adapter: String,
    pub source: String,
}
