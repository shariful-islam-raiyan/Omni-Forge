//! Ties `omni-core` (discovery/resolution) and `omni-adapter-api` (adapter
//! execution) together into one target build, including cache lookup and
//! build-report assembly. `build_target` itself builds exactly one target,
//! sequentially; it is safe to call concurrently for independent targets
//! from multiple threads (see `commands::build`'s `build.parallel` path) —
//! each target's cache entries and output directory are scoped under its
//! own name (`.omniforge/cache/<target>/...`, `dist/<target>/...`), so
//! concurrent targets cannot collide on disk.

use omni_adapter_api::{Adapter, BuildContext};
use omni_core::cache::{BuildCache, CacheKeyBuilder};
use omni_core::diagnostics::Diagnostic;
use omni_core::fsutil::sha256_file;
use omni_core::loader::LoadedProject;
use omni_core::paths::{safe_join, OutputDir};
use omni_core::resolver::{self, ResolvedComponent};
use omni_core::spec::adapter::AdapterManifest;
use omni_core::spec::build_report::{
    AdapterRef, ArtifactRecord, CacheResult, TargetReport, TargetStatus,
};
use omni_core::spec::component::ComponentManifest;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub enum Mode {
    ValidateOnly,
    Compile,
}

pub fn build_target(
    loaded: &LoadedProject,
    target_name: &str,
    components: &[ComponentManifest],
    adapters: &[Box<dyn Adapter + Send + Sync>],
    mode: Mode,
) -> TargetReport {
    let Some(target_cfg) = loaded.file.targets.get(target_name) else {
        return skipped(
            target_name,
            Diagnostic::error(
                "E-BUILD-UNKNOWN-TARGET",
                format!("target '{target_name}' is not defined in omni.yaml"),
            ),
        );
    };

    if !target_cfg.enabled {
        return skipped(
            target_name,
            Diagnostic::info(
                "I-BUILD-TARGET-DISABLED",
                format!("target '{target_name}' is disabled"),
            ),
        );
    }

    let Some(adapter_id) = &target_cfg.adapter else {
        return failed(
            target_name,
            None,
            Diagnostic::error(
                "E-BUILD-MISSING-ADAPTER-CONFIG",
                format!("target '{target_name}' is enabled but has no adapter configured"),
            ),
        );
    };

    let Some(adapter) = adapters.iter().find(|a| &a.manifest().id == adapter_id) else {
        return unsupported(target_name, Diagnostic::error(
            "E-BUILD-ADAPTER-UNAVAILABLE",
            format!(
                "target '{target_name}' requires adapter '{adapter_id}', which is not available in this build of OmniForge"
            ),
        ).with_target(target_name).with_suggestion("run 'omni adapter list' to see available adapters"));
    };

    let adapter_manifests: Vec<AdapterManifest> =
        adapters.iter().map(|a| a.manifest().clone()).collect();

    // Only components that actually declare an implementation for this
    // target are candidate roots; components meant for other targets are
    // simply not part of this target's graph.
    let roots: Vec<String> = components
        .iter()
        .filter(|c| c.implementations.contains_key(target_name))
        .map(|c| c.name.clone())
        .collect();

    if roots.is_empty() {
        return failed(
            target_name,
            Some(adapter.manifest()),
            Diagnostic::error(
                "E-BUILD-NO-COMPONENTS",
                format!(
                    "no discovered component declares an implementation for target '{target_name}'"
                ),
            )
            .with_target(target_name),
        );
    }

    let plan = match resolver::resolve(components, &adapter_manifests, target_name, &roots) {
        Ok(plan) => plan,
        Err(diags) => {
            return TargetReport {
                target: target_name.to_string(),
                adapter: Some(adapter_ref(adapter.manifest())),
                status: TargetStatus::Failed,
                cache: None,
                artifacts: Vec::new(),
                diagnostics: diags.into_items(),
            };
        }
    };

    let entry_path = match &target_cfg.entry {
        Some(rel) => match safe_join(&loaded.root, rel) {
            Ok(p) => Some(p),
            Err(e) => {
                return failed(
                    target_name,
                    Some(adapter.manifest()),
                    Diagnostic::error("E-BUILD-UNSAFE-ENTRY", format!("{e}"))
                        .with_target(target_name),
                );
            }
        },
        None => None,
    };

    // A real dependency edge from this target to its declared API contract
    // (see `docs/decisions/0009-unified-dev-process-lifecycle.md`): the
    // contract is schema-validated as part of *this* build (previously it
    // was only ever checked by a hand-written integration test, never by
    // `omni build`/`omni check`), and its bytes feed the cache key below so
    // editing the contract actually invalidates this target's cache.
    let contract_bytes = match &target_cfg.contract {
        Some(rel) => match safe_join(&loaded.root, rel) {
            Ok(path) => match fs::read(&path) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    if let Err(diags) = omni_core::spec::api_contract::parse(&text, rel) {
                        return TargetReport {
                            target: target_name.to_string(),
                            adapter: Some(adapter_ref(adapter.manifest())),
                            status: TargetStatus::Failed,
                            cache: None,
                            artifacts: Vec::new(),
                            diagnostics: diags,
                        };
                    }
                    Some((rel.clone(), bytes))
                }
                Err(e) => {
                    return failed(
                        target_name,
                        Some(adapter.manifest()),
                        Diagnostic::error(
                            "E-BUILD-CONTRACT-UNREADABLE",
                            format!("could not read declared contract '{rel}': {e}"),
                        )
                        .with_target(target_name),
                    );
                }
            },
            Err(e) => {
                return failed(
                    target_name,
                    Some(adapter.manifest()),
                    Diagnostic::error("E-BUILD-UNSAFE-CONTRACT", format!("{e}"))
                        .with_target(target_name),
                );
            }
        },
        None => None,
    };

    let per_target_output_rel = format!("{}/{}", loaded.file.build.output, target_name);
    let output = match OutputDir::resolve(&loaded.root, &per_target_output_rel) {
        Ok(o) => o,
        Err(e) => {
            return failed(
                target_name,
                Some(adapter.manifest()),
                Diagnostic::error("E-BUILD-OUTPUT", format!("{e}")).with_target(target_name),
            );
        }
    };

    let ctx = BuildContext {
        project_root: loaded.root.clone(),
        target: target_name.to_string(),
        entry_path,
        output: output.clone(),
        components: plan.components,
    };

    if matches!(mode, Mode::ValidateOnly) {
        let diags = adapter.validate(&ctx);
        return TargetReport {
            target: target_name.to_string(),
            adapter: Some(adapter_ref(adapter.manifest())),
            status: if diags.has_errors() {
                TargetStatus::Failed
            } else {
                TargetStatus::Success
            },
            cache: None,
            artifacts: Vec::new(),
            diagnostics: diags.into_items(),
        };
    }

    let cache_enabled = loaded.file.build.cache;
    let cache = BuildCache::open(&loaded.root).ok();

    let key = compute_cache_key(
        target_name,
        &ctx.components,
        adapter.manifest(),
        ctx.entry_path.as_deref(),
        &loaded.root,
        contract_bytes.as_ref(),
    );

    if cache_enabled {
        if let (Some(cache), Ok(key)) = (&cache, &key) {
            if let Ok(true) = cache.restore(target_name, key, output.path()) {
                let artifacts = collect_artifacts(output.path());
                return TargetReport {
                    target: target_name.to_string(),
                    adapter: Some(adapter_ref(adapter.manifest())),
                    status: TargetStatus::Success,
                    cache: Some(CacheResult::Hit),
                    artifacts,
                    diagnostics: vec![Diagnostic::info(
                        "I-BUILD-CACHE-HIT",
                        "restored from local build cache",
                    )],
                };
            }
        }
    }

    let result = adapter.compile(&ctx);
    if result.diagnostics.has_errors() {
        return TargetReport {
            target: target_name.to_string(),
            adapter: Some(adapter_ref(adapter.manifest())),
            status: TargetStatus::Failed,
            cache: if cache_enabled {
                Some(CacheResult::Miss)
            } else {
                Some(CacheResult::Disabled)
            },
            artifacts: Vec::new(),
            diagnostics: result.diagnostics.into_items(),
        };
    }

    let artifacts: Vec<ArtifactRecord> = result
        .artifacts
        .iter()
        .filter_map(|a| artifact_record(a.absolute_path.as_path(), &a.relative_path))
        .collect();

    if artifacts.len() != result.artifacts.len() {
        let mut diags = result.diagnostics;
        diags.push(Diagnostic::error(
            "E-BUILD-ARTIFACT-VERIFY",
            "one or more reported artifacts could not be re-read from disk for verification",
        ));
        return TargetReport {
            target: target_name.to_string(),
            adapter: Some(adapter_ref(adapter.manifest())),
            status: TargetStatus::Failed,
            cache: Some(CacheResult::Miss),
            artifacts: Vec::new(),
            diagnostics: diags.into_items(),
        };
    }

    if cache_enabled {
        if let (Some(cache), Ok(key)) = (&cache, &key) {
            let _ = cache.store(target_name, key, output.path());
        }
    }

    TargetReport {
        target: target_name.to_string(),
        adapter: Some(adapter_ref(adapter.manifest())),
        status: TargetStatus::Success,
        cache: Some(if cache_enabled {
            CacheResult::Miss
        } else {
            CacheResult::Disabled
        }),
        artifacts,
        diagnostics: result.diagnostics.into_items(),
    }
}

fn adapter_ref(m: &AdapterManifest) -> AdapterRef {
    AdapterRef {
        id: m.id.clone(),
        version: m.version.clone(),
    }
}

fn skipped(target: &str, d: Diagnostic) -> TargetReport {
    TargetReport {
        target: target.to_string(),
        adapter: None,
        status: TargetStatus::Skipped,
        cache: None,
        artifacts: Vec::new(),
        diagnostics: vec![d],
    }
}

fn unsupported(target: &str, d: Diagnostic) -> TargetReport {
    TargetReport {
        target: target.to_string(),
        adapter: None,
        status: TargetStatus::Unsupported,
        cache: None,
        artifacts: Vec::new(),
        diagnostics: vec![d],
    }
}

fn failed(target: &str, adapter: Option<&AdapterManifest>, d: Diagnostic) -> TargetReport {
    TargetReport {
        target: target.to_string(),
        adapter: adapter.map(adapter_ref),
        status: TargetStatus::Failed,
        cache: None,
        artifacts: Vec::new(),
        diagnostics: vec![d],
    }
}

fn artifact_record(path: &Path, relative: &str) -> Option<ArtifactRecord> {
    let bytes = fs::metadata(path).ok()?.len();
    let sha256 = sha256_file(path).ok()?;
    Some(ArtifactRecord {
        path: relative.to_string(),
        bytes,
        sha256,
    })
}

fn collect_artifacts(dir: &Path) -> Vec<ArtifactRecord> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(dir).unwrap_or(entry.path());
            if let Some(record) =
                artifact_record(entry.path(), &rel.to_string_lossy().replace('\\', "/"))
            {
                out.push(record);
            }
        }
    }
    out
}

fn compute_cache_key(
    target: &str,
    components: &[ResolvedComponent],
    adapter: &AdapterManifest,
    entry_path: Option<&Path>,
    project_root: &Path,
    contract: Option<&(String, Vec<u8>)>,
) -> Result<String, ()> {
    let mut key = CacheKeyBuilder::new();
    key.add_str("target", target);
    key.add_str("adapter_id", &adapter.id);
    key.add_str("adapter_version", &adapter.version);

    if let Some(entry) = entry_path {
        let bytes = fs::read(entry).map_err(|_| ())?;
        key.add("entry", &bytes);
    }

    if let Some((rel, bytes)) = contract {
        key.add_str("contract_path", rel);
        key.add("contract_bytes", bytes);
    }

    let mut sorted: BTreeMap<&str, &ResolvedComponent> = BTreeMap::new();
    for c in components {
        sorted.insert(c.name.as_str(), c);
    }
    for (name, c) in sorted {
        key.add_str("component_name", name);
        key.add_str("component_version", &c.version);
        let manifest_path = c.manifest_dir.join("component.yaml");
        let manifest_bytes = fs::read(&manifest_path).map_err(|_| ())?;
        key.add("component_manifest", &manifest_bytes);
        if let Ok(source_path) = safe_join(&c.manifest_dir, &c.implementation.source) {
            if let Ok(bytes) = fs::read(&source_path) {
                key.add("component_source", &bytes);
            }
        }
    }

    let public_dir = project_root.join("public");
    if public_dir.is_dir() {
        let mut files: Vec<_> = walkdir::WalkDir::new(&public_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();
        files.sort();
        for f in files {
            let rel = f
                .strip_prefix(&public_dir)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            key.add_str("public_file", &rel);
            if let Ok(bytes) = fs::read(&f) {
                key.add("public_content", &bytes);
            }
        }
    }

    Ok(key.finish())
}
