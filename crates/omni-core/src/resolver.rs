//! Resolves which component implementation to use for a given target,
//! checking adapter capabilities and dependency integrity before a build is
//! allowed to proceed.

use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::graph::DependencyGraph;
use crate::spec::adapter::AdapterManifest;
use crate::spec::component::{ComponentImplementation, ComponentManifest};

#[derive(Debug, Clone)]
pub struct ResolvedComponent {
    pub name: String,
    pub version: String,
    pub kind: crate::spec::component::ComponentKind,
    pub implementation: ComponentImplementation,
    pub manifest_dir: std::path::PathBuf,
    pub adapter: AdapterManifest,
}

#[derive(Debug)]
pub struct ResolvedBuildPlan {
    /// Dependency-first order: a component never appears before something it
    /// depends on.
    pub components: Vec<ResolvedComponent>,
}

/// Resolves `roots` (and everything they transitively depend on) for
/// `target`, choosing implementations and validating adapter capabilities.
/// Returns `Err` only when resolution could not produce *any* usable plan;
/// recoverable-but-worth-surfacing information is returned as diagnostics
/// even on `Ok`.
pub fn resolve(
    components: &[ComponentManifest],
    adapters: &[AdapterManifest],
    target: &str,
    roots: &[String],
) -> Result<ResolvedBuildPlan, Diagnostics> {
    let mut diags = Diagnostics::new();
    let by_name: std::collections::BTreeMap<&str, &ComponentManifest> =
        components.iter().map(|c| (c.name.as_str(), c)).collect();
    let by_adapter_id: std::collections::BTreeMap<&str, &AdapterManifest> =
        adapters.iter().map(|a| (a.id.as_str(), a)).collect();

    let mut graph = DependencyGraph::new();
    let mut resolved_map: std::collections::BTreeMap<String, ResolvedComponent> =
        std::collections::BTreeMap::new();
    let mut to_visit: Vec<String> = roots.to_vec();
    let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut ok = true;

    while let Some(name) = to_visit.pop() {
        if visited.contains(&name) {
            continue;
        }
        visited.insert(name.clone());
        graph.add_node(&name);

        let Some(manifest) = by_name.get(name.as_str()) else {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-MISSING-COMPONENT",
                    format!("component '{name}' was requested but is not defined in this project"),
                )
                .with_component(name.clone())
                .with_target(target)
                .with_suggestion("check the component name and its component.yaml location"),
            );
            ok = false;
            continue;
        };

        let Some(implementation) = manifest.implementations.get(target) else {
            let available: Vec<&str> = manifest
                .implementations
                .keys()
                .map(|s| s.as_str())
                .collect();
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-NO-IMPLEMENTATION",
                    format!(
                        "component '{name}' has no implementation for target '{target}'. Available: [{}]",
                        available.join(", ")
                    ),
                )
                .with_component(name.clone())
                .with_target(target)
                .with_suggestion(format!("add an implementation for target '{target}', or disable that target")),
            );
            ok = false;
            continue;
        };

        let Some(adapter) = by_adapter_id.get(implementation.adapter.as_str()) else {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-NO-ADAPTER",
                    format!(
                        "component '{name}' implementation for target '{target}' references unknown adapter '{}'",
                        implementation.adapter
                    ),
                )
                .with_component(name.clone())
                .with_target(target),
            );
            ok = false;
            continue;
        };

        let kind_str = manifest.kind.to_string();
        if !adapter.capabilities.supports_target(target) {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-CAPABILITY",
                    format!(
                        "adapter '{}' does not declare support for target '{target}'",
                        adapter.id
                    ),
                )
                .with_component(name.clone())
                .with_target(target),
            );
            ok = false;
            continue;
        }
        if !adapter.capabilities.supports_kind(&kind_str) {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-CAPABILITY",
                    format!(
                        "adapter '{}' does not declare support for component kind '{kind_str}' (component '{name}')",
                        adapter.id
                    ),
                )
                .with_component(name.clone())
                .with_target(target),
            );
            ok = false;
            continue;
        }
        if !adapter.capabilities.supports_operation("compile") {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-CAPABILITY",
                    format!(
                        "adapter '{}' does not declare the 'compile' operation",
                        adapter.id
                    ),
                )
                .with_component(name.clone())
                .with_target(target),
            );
            ok = false;
            continue;
        }

        for dep in &manifest.dependencies {
            graph.add_edge(&name, dep);
            if !visited.contains(dep) {
                to_visit.push(dep.clone());
            }
        }

        resolved_map.insert(
            name.clone(),
            ResolvedComponent {
                name: name.clone(),
                version: manifest.version.clone(),
                kind: manifest.kind,
                implementation: implementation.clone(),
                manifest_dir: manifest.manifest_dir.clone(),
                adapter: (*adapter).clone(),
            },
        );
    }

    if !ok {
        return Err(diags);
    }

    let order = match graph.topological_order() {
        Ok(order) => order,
        Err(cycle) => {
            diags.push(
                Diagnostic::error(
                    "E-RESOLVE-CYCLE",
                    format!("dependency cycle detected: {}", cycle.join(" -> ")),
                )
                .with_target(target),
            );
            return Err(diags);
        }
    };

    let ordered_components: Vec<ResolvedComponent> = order
        .into_iter()
        .filter_map(|name| resolved_map.remove(&name))
        .collect();

    Ok(ResolvedBuildPlan {
        components: ordered_components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::adapter::{AdapterCapabilities, AdapterManifest};
    use crate::spec::component::{
        ComponentImplementation, ComponentInterface, ComponentKind, ComponentManifest,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn web_adapter() -> AdapterManifest {
        AdapterManifest {
            schema: "omniforge.dev/adapter/v1".into(),
            id: "web-static".into(),
            version: "0.1.0".into(),
            protocol: "in-process/v1".into(),
            capabilities: AdapterCapabilities {
                languages: vec!["template".into()],
                component_kinds: vec!["ui".into()],
                targets: vec!["web".into()],
                operations: vec!["validate".into(), "compile".into(), "package".into()],
                requires: vec![],
            },
            dev: None,
        }
    }

    fn component(name: &str, deps: Vec<&str>, targets: Vec<&str>) -> ComponentManifest {
        let mut implementations = BTreeMap::new();
        for t in targets {
            implementations.insert(
                t.to_string(),
                ComponentImplementation {
                    adapter: "web-static".into(),
                    source: "x.html.tmpl".into(),
                },
            );
        }
        ComponentManifest {
            schema: "omniforge.dev/component/v1".into(),
            name: name.into(),
            version: "1.0.0".into(),
            kind: ComponentKind::Ui,
            dependencies: deps.into_iter().map(String::from).collect(),
            composes: BTreeMap::new(),
            interface: ComponentInterface::default(),
            implementations,
            manifest_path: PathBuf::from("component.yaml"),
            manifest_dir: PathBuf::from("."),
        }
    }

    #[test]
    fn resolves_single_component() {
        let comps = vec![component("ui.home", vec![], vec!["web"])];
        let adapters = vec![web_adapter()];
        let plan = resolve(&comps, &adapters, "web", &["ui.home".to_string()]).unwrap();
        assert_eq!(plan.components.len(), 1);
        assert_eq!(plan.components[0].name, "ui.home");
    }

    #[test]
    fn missing_component_is_reported() {
        let comps = vec![];
        let adapters = vec![web_adapter()];
        let err = resolve(&comps, &adapters, "web", &["ui.home".to_string()]).unwrap_err();
        assert!(err
            .items()
            .iter()
            .any(|d| d.code == "E-RESOLVE-MISSING-COMPONENT"));
    }

    #[test]
    fn missing_implementation_for_target_is_reported() {
        let comps = vec![component("ui.home", vec![], vec!["android"])];
        let adapters = vec![web_adapter()];
        let err = resolve(&comps, &adapters, "web", &["ui.home".to_string()]).unwrap_err();
        assert!(err
            .items()
            .iter()
            .any(|d| d.code == "E-RESOLVE-NO-IMPLEMENTATION"));
    }

    #[test]
    fn dependency_cycle_is_detected() {
        let mut a = component("a", vec!["b"], vec!["web"]);
        let b = component("b", vec!["a"], vec!["web"]);
        a.dependencies = vec!["b".into()];
        let comps = vec![a, b];
        let adapters = vec![web_adapter()];
        let err = resolve(&comps, &adapters, "web", &["a".to_string()]).unwrap_err();
        assert!(err.items().iter().any(|d| d.code == "E-RESOLVE-CYCLE"));
    }

    #[test]
    fn resolves_transitive_dependency_in_order() {
        let a = component("a", vec!["b"], vec!["web"]);
        let b = component("b", vec![], vec!["web"]);
        let comps = vec![a, b];
        let adapters = vec![web_adapter()];
        let plan = resolve(&comps, &adapters, "web", &["a".to_string()]).unwrap();
        let names: Vec<&str> = plan.components.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a"]);
    }

    #[test]
    fn unknown_adapter_capability_mismatch_is_reported() {
        let comps = vec![component("svc.x", vec![], vec!["web"]).tap_kind(ComponentKind::Service)];
        let adapters = vec![web_adapter()];
        let err = resolve(&comps, &adapters, "web", &["svc.x".to_string()]).unwrap_err();
        assert!(err.items().iter().any(|d| d.code == "E-RESOLVE-CAPABILITY"));
    }

    trait TapKind {
        fn tap_kind(self, kind: ComponentKind) -> Self;
    }
    impl TapKind for ComponentManifest {
        fn tap_kind(mut self, kind: ComponentKind) -> Self {
            self.kind = kind;
            self
        }
    }
}
