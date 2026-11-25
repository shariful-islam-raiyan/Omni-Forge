use crate::diag_print;
use crate::orchestrator::{self, Mode};
use crate::registry;
use omni_core::loader;
use std::path::Path;

pub fn run(cwd: &Path, target: Option<&str>, json: bool) -> i32 {
    let root = match loader::find_project_root(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let (loaded, diags) = match loader::load_project(&root) {
        Ok(ok) => ok,
        Err(omni_core::OmniError::Invalid(items)) => {
            diag_print::print_all(&items);
            eprintln!(
                "\n{} error(s) found in omni.yaml",
                items.iter().filter(|d| d.is_error()).count()
            );
            return 1;
        }
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
    let target_names: Vec<String> = match target {
        Some(t) => vec![t.to_string()],
        None => loaded
            .file
            .targets
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(name, _)| name.clone())
            .collect(),
    };

    if target_names.is_empty() {
        println!("No enabled targets to check.");
        return 0;
    }

    let mut reports = Vec::new();
    let mut ok = true;
    for t in &target_names {
        let report =
            orchestrator::build_target(&loaded, t, &components, &adapters, Mode::ValidateOnly);
        diag_print::print_all(&report.diagnostics);
        let status_ok = matches!(
            report.status,
            omni_core::spec::build_report::TargetStatus::Success
        );
        println!("target '{t}': {}", if status_ok { "OK" } else { "FAILED" });
        ok = ok && status_ok;
        reports.push(report);
    }

    if json {
        let payload = serde_json::json!({ "targets": reports });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    }

    if ok {
        0
    } else {
        1
    }
}
