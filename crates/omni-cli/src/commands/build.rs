use crate::diag_print;
use crate::orchestrator::{self, Mode};
use crate::registry;
use omni_core::loader;
use omni_core::spec::build_report::{BuildReport, ProjectRef, TargetReport, TargetStatus};
use omni_core::spec::project::KNOWN_TARGETS;
use std::path::Path;

pub fn run(cwd: &Path, target_arg: &str, allow_partial: bool) -> i32 {
    let root = match loader::find_project_root(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let (loaded, _diags) = match loader::load_project(&root) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

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

    let target_names: Vec<String> = if target_arg == "all" {
        loaded
            .file
            .targets
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(n, _)| n.clone())
            .collect()
    } else {
        if !KNOWN_TARGETS.contains(&target_arg) {
            eprintln!("error: E-BUILD-UNKNOWN-TARGET: '{target_arg}' is not a known OmniForge target.\n  known targets: {}", KNOWN_TARGETS.join(", "));
            return 1;
        }
        vec![target_arg.to_string()]
    };

    if target_names.is_empty() {
        eprintln!("error: E-BUILD-NO-TARGETS: no targets are enabled in omni.yaml");
        return 1;
    }

    let mut reports: Vec<TargetReport> = Vec::new();
    for t in &target_names {
        println!("==> building target '{t}'");
        let report = orchestrator::build_target(&loaded, t, &components, &adapters, Mode::Compile);
        diag_print::print_all(&report.diagnostics);
        match report.status {
            TargetStatus::Success => {
                let cache_note = match report.cache {
                    Some(omni_core::spec::build_report::CacheResult::Hit) => " (cache hit)",
                    Some(omni_core::spec::build_report::CacheResult::Miss) => "",
                    _ => "",
                };
                println!(
                    "    target '{t}': SUCCESS{cache_note} ({} artifact(s))",
                    report.artifacts.len()
                );
            }
            TargetStatus::Failed => println!("    target '{t}': FAILED"),
            TargetStatus::Unsupported => println!("    target '{t}': UNSUPPORTED"),
            TargetStatus::Skipped => println!("    target '{t}': SKIPPED"),
            TargetStatus::Partial => println!("    target '{t}': PARTIAL"),
        }
        reports.push(report);
    }

    let build_report = BuildReport::new(
        ProjectRef {
            name: loaded.file.project.name.clone(),
            version: loaded.file.project.version.clone(),
        },
        reports,
    );

    let report_path = root
        .join(&loaded.file.build.output)
        .join("build-report.json");
    if let Some(parent) = report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&build_report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&report_path, json) {
                eprintln!(
                    "warning: could not write build report to '{}': {e}",
                    report_path.display()
                );
            } else {
                println!("\nbuild report written to {}", report_path.display());
            }
        }
        Err(e) => eprintln!("warning: could not serialize build report: {e}"),
    }

    let any_failed = build_report
        .targets
        .iter()
        .any(|t| t.status == TargetStatus::Failed);
    let any_unsupported = build_report
        .targets
        .iter()
        .any(|t| t.status == TargetStatus::Unsupported);

    if any_failed {
        eprintln!("\nbuild FAILED: one or more targets failed.");
        return 1;
    }
    if any_unsupported && !allow_partial {
        eprintln!("\nbuild FAILED: one or more enabled targets have no available adapter.\n  pass --allow-partial to accept a partial build.");
        return 1;
    }
    if any_unsupported {
        println!(
            "\nbuild PARTIAL: some enabled targets were unsupported (--allow-partial was set)."
        );
    } else {
        println!("\nbuild SUCCESS");
    }
    0
}
