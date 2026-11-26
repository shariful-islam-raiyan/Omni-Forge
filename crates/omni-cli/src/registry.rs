use crate::adapters::python_backend;
use omni_adapter_api::Adapter;
use omni_android_adapter::AndroidGradleAdapter;
use omni_web_adapter::WebStaticAdapter;

/// The adapters statically linked into this build of the CLI. OmniForge does
/// not dynamically load or execute third-party adapter code today (see
/// docs/security-model.md) — every adapter here is part of this repository
/// and was reviewed with it. `web-static` runs in-process; `python-backend`
/// runs out-of-process (see `omni_adapter_api::process`) but is just as
/// statically known to this binary — its script is embedded at compile time,
/// not discovered or downloaded at runtime. `android-gradle` is in-process
/// Rust that safely shells out to a real, locally-discovered Gradle (see
/// `docs/decisions/0011-android-gradle-adapter.md`) — always registered
/// regardless of whether this machine's Android toolchain is actually
/// present; a missing toolchain is reported at build/doctor time
/// (`E-ANDROID-TOOLCHAIN-MISSING`), not by omitting the adapter.
pub fn built_in_adapters() -> Vec<Box<dyn Adapter>> {
    vec![
        Box::new(WebStaticAdapter::new()),
        Box::new(python_backend::build()),
        Box::new(AndroidGradleAdapter::new()),
    ]
}
