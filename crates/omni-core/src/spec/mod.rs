pub mod adapter;
pub mod api_contract;
pub mod build_report;
pub mod component;
pub mod project;

pub use adapter::{AdapterCapabilities, AdapterManifest};
pub use api_contract::{
    ApiContract, ApiInfo, ApiOperation, ApiRequest, ApiResponse, API_CONTRACT_SCHEMA_VERSION,
};
pub use build_report::{ArtifactRecord, BuildReport, CacheResult, TargetReport, TargetStatus};
pub use component::{
    ComponentImplementation, ComponentInterface, ComponentKind, ComponentManifest,
};
pub use project::{BuildConfig, ProjectFile, ProjectInfo, SecurityConfig, TargetConfig};
