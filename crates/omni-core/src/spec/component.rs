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
    /// Named composition slots: each key is a placeholder name usable in
    /// this component's own template (alongside `interface.inputs`), whose
    /// value is the already-rendered, already-escaped HTML of another
    /// component. See `docs/decisions/0010-component-composition-via-slots.md`.
    #[serde(default)]
    pub composes: BTreeMap<String, ComposeSlot>,
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

/// One composition slot: `component` names the child component to render
/// into this slot (must also appear in this manifest's `dependencies` — see
/// `components.rs::discover_components`); `inputs` are passed to that child
/// the same way a webapp manifest's `root.inputs` are passed to a root
/// component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeSlot {
    pub component: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
}
