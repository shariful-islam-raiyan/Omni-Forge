use crate::diag_print;
use crate::registry;
use omni_core::loader;
use std::path::Path;

/// `omni test` validates the project (equivalent to `omni check`) and then,
/// for each enabled target, asks its adapter to run its `test` operation.
///
/// As of this version no shipped adapter declares the `test` capability, so
/// this command never claims tests ran and passed when none did — it
/// reports validation results and states plainly that no test operation is
/// available, rather than faking a result.
pub fn run(cwd: &Path) -> i32 {
    let root = match loader::find_project_root(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let (loaded, diags) = match loader::load_project(&root) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    diag_print::print_all(diags.items());

    let (components, comp_diags) = match omni_core::components::discover_components(
        &root,
        &loaded.file.components.directory,
    ) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    diag_print::print_all(comp_diags.items());
    if comp_diags.has_errors() {
        return 1;
    }

    let adapters = registry::built_in_adapters();
    let enabled: Vec<String> = loaded
        .file
        .targets
        .iter()
        .filter(|(_, c)| c.enabled)
        .map(|(n, _)| n.clone())
        .collect();

    if enabled.is_empty() {
        println!("No enabled targets; nothing to test.");
        return 0;
    }

    let mut any_capable = false;
    for t in &enabled {
        let Some(cfg) = loaded.file.targets.get(t) else {
            continue;
        };
        let Some(adapter_id) = &cfg.adapter else {
            continue;
        };
        let Some(adapter) = adapters.iter().find(|a| &a.manifest().id == adapter_id) else {
            println!("target '{t}': adapter '{adapter_id}' unavailable, skipping");
            continue;
        };
        if adapter.manifest().capabilities.supports_operation("test") {
            any_capable = true;
            // Not reached today: no shipped adapter declares 'test'. Left in
            // place so a future adapter that does gets picked up here
            // instead of requiring CLI changes.
            println!("target '{t}': adapter '{adapter_id}' declares 'test' support but no execution path is implemented yet");
        } else {
            println!(
                "target '{t}': adapter '{adapter_id}' does not support the 'test' operation (supported: {})",
                adapter.manifest().capabilities.operations.join(", ")
            );
        }
    }
    let _ = components;

    if !any_capable {
        println!(
            "\nNo tests were run: no enabled target's adapter implements a 'test' operation yet."
        );
    }
    0
}
