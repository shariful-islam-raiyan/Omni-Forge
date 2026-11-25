use crate::adapters::python_backend;
use omni_adapter_api::Adapter;
use omni_web_adapter::WebStaticAdapter;

/// The adapters statically linked into this build of the CLI. OmniForge does
/// not dynamically load or execute third-party adapter code today (see
/// docs/security-model.md) — every adapter here is part of this repository
/// and was reviewed with it. `web-static` runs in-process; `python-backend`
/// runs out-of-process (see `omni_adapter_api::process`) but is just as
/// statically known to this binary — its script is embedded at compile time,
/// not discovered or downloaded at runtime.
pub fn built_in_adapters() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(WebStaticAdapter::new()),
        Box::new(python_backend::build()),
    ]
}
