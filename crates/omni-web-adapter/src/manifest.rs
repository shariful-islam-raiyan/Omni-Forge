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
    #[serde(default)]
    pub seo: SeoConfig,
}

/// All fields optional, by design — see `docs/seo.md` for exactly what
/// each one produces and `web-static`'s honest single-page scope (one
/// rendered page, so no per-page duplicate-title/description checking,
/// and `sitemap.xml` lists exactly one URL).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeoConfig {
    #[serde(default)]
    pub description: Option<String>,
    /// Absolute base URL this page is deployed at (e.g.
    /// `https://example.com`). Required to generate `sitemap.xml` and the
    /// default canonical URL — without it, both are honestly skipped
    /// rather than written with a guessed or relative URL.
    #[serde(default)]
    pub site_url: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub og_image: Option<String>,
    #[serde(default)]
    pub robots: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
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
