//! The real, unified `omni dev` orchestration logic.
//!
//! Unlike `omni build`, which only ever runs bounded, run-to-completion
//! adapter operations, `omni dev` additionally starts and supervises real
//! long-running child processes (see `omni_adapter_api::devserver`) for any
//! target whose adapter declares a dev-serve capability, and serves any
//! in-process target (today: `web-static`) as static files over HTTP on a
//! background thread. It prints a `READY <target> <url>` line per target
//! that actually starts serving, then blocks until told to shut down,
//! stopping every managed child process cleanly before returning.
//!
//! Split into [`start`] (build + start serving, returns a [`DevSession`]
//! describing exactly what's running and on which ports) and
//! [`DevSession::wait_and_shutdown`] (block for a shutdown signal, then
//! stop everything) so tests can drive this deterministically — ports are
//! often auto-picked, so a test needs the real resolved values, not a
//! stdout-scraping guess.

use crate::diag_print;
use crate::http;
use crate::orchestrator::{self, Mode};
use crate::registry;
use omni_adapter_api::{ManagedProcess, StopOutcome};
use omni_core::diagnostics::Diagnostic;
use omni_core::loader;
use omni_core::spec::build_report::{TargetReport, TargetStatus};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// How `DevSession::wait_and_shutdown` learns it's time to stop.
pub enum ShutdownSignal {
    /// Install a real Ctrl-C/Ctrl-Break handler (the real CLI's choice).
    CtrlC,
    /// Shut down as soon as a message arrives on this channel (what tests
    /// use, so they control the exact moment deterministically).
    Channel(mpsc::Receiver<()>),
}

enum Handle {
    /// A background thread serving `dist/<target>` as static files.
    Static(http::StaticServer),
    Process {
        process: ManagedProcess,
        stop_grace: Duration,
    },
    /// A managed process that exited on its own while `omni dev` was
    /// running; already reported once, nothing further to poll or stop.
    Crashed,
}

/// One target that is actually being served right now.
pub struct TargetHandle {
    pub target: String,
    pub port: u16,
    handle: Handle,
}

impl TargetHandle {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// The result of [`start`]: which targets ended up being served, plus every
/// target's build report (so a caller/test can inspect why a target was
/// skipped).
pub struct DevSession {
    pub handles: Vec<TargetHandle>,
    pub reports: Vec<TargetReport>,
}

impl DevSession {
    /// Blocks until `shutdown` fires, then stops every managed process
    /// (graceful-then-hard-kill, see `devserver::ManagedProcess::stop`) and
    /// returns a process exit code. While waiting, polls every managed
    /// process so an unexpected exit is reported instead of silently
    /// hanging; other targets keep running.
    pub fn wait_and_shutdown(mut self, shutdown: ShutdownSignal) -> i32 {
        let shutdown_rx = match shutdown {
            ShutdownSignal::Channel(rx) => rx,
            ShutdownSignal::CtrlC => {
                let (tx, rx) = mpsc::channel();
                if ctrlc::set_handler(move || {
                    let _ = tx.send(());
                })
                .is_err()
                {
                    eprintln!(
                        "warning: could not install a Ctrl-C handler; stop targets with Ctrl-\\ or by closing this terminal"
                    );
                }
                rx
            }
        };

        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            for h in &mut self.handles {
                let crashed = if let Handle::Process { process, .. } = &mut h.handle {
                    process.poll_exit()
                } else {
                    None
                };
                if let Some(status) = crashed {
                    eprintln!(
                        "warning: target '{}' dev server exited unexpectedly ({status})",
                        h.target
                    );
                    h.handle = Handle::Crashed;
                }
            }
            thread::sleep(SUPERVISOR_POLL_INTERVAL);
        }

        println!("\nshutting down...");
        for h in self.handles.drain(..) {
            match h.handle {
                Handle::Process {
                    mut process,
                    stop_grace,
                } => {
                    println!("  stopping '{}'...", h.target);
                    match process.stop(stop_grace) {
                        StopOutcome::Graceful => println!("  stopped '{}' (graceful)", h.target),
                        StopOutcome::HardKilled => {
                            println!("  stopped '{}' (hard-killed after grace period)", h.target)
                        }
                        StopOutcome::AlreadyExited => {
                            println!("  '{}' had already exited", h.target)
                        }
                    }
                }
                Handle::Static(server) => {
                    println!("  stopping '{}'...", h.target);
                    server.stop();
                    println!("  stopped '{}'", h.target);
                }
                Handle::Crashed => {}
            }
        }
        println!("all dev servers stopped.");
        0
    }
}

/// Builds every requested (or, if empty, every enabled) target and starts
/// serving each successfully-built one. Returns `Err` only for a hard
/// failure that means nothing could even be attempted (bad project, an
/// explicit port conflict); a target-level failure (bad build, missing
/// toolchain, adapter without dev-serve support) is reported as a
/// diagnostic/log line and simply isn't included in `DevSession::handles` —
/// `omni dev` is a best-effort "show me what's working" command.
pub fn start(
    cwd: &Path,
    requested_targets: &[String],
    port_overrides: &[(String, u16)],
) -> Result<DevSession, i32> {
    start_with_adapters(
        cwd,
        requested_targets,
        port_overrides,
        registry::built_in_adapters(),
    )
}

/// Same as [`start`], but with an explicit adapter list instead of
/// `registry::built_in_adapters()` -- lets tests substitute, e.g., a
/// `python-backend` built with a deliberately bogus launcher candidate
/// list to exercise the "toolchain missing" path deterministically,
/// without touching the real process environment (`PATH`).
pub fn start_with_adapters(
    cwd: &Path,
    requested_targets: &[String],
    port_overrides: &[(String, u16)],
    adapters: Vec<Box<dyn omni_adapter_api::Adapter + Send + Sync>>,
) -> Result<DevSession, i32> {
    let root = loader::find_project_root(cwd).map_err(|e| {
        eprintln!("error: {e}");
        1
    })?;
    let (loaded, _diags) = loader::load_project(&root).map_err(|e| {
        eprintln!("error: {e}");
        1
    })?;
    let (components, comp_diags) =
        omni_core::components::discover_components(&root, &loaded.file.components.directory)
            .map_err(|e| {
                eprintln!("error: {e}");
                1
            })?;
    if comp_diags.has_errors() {
        diag_print::print_all(comp_diags.items());
        return Err(1);
    }

    let target_names: Vec<String> = if requested_targets.is_empty() {
        loaded
            .file
            .targets
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        for t in requested_targets {
            if !loaded.file.targets.contains_key(t) {
                eprintln!("error: E-DEV-UNKNOWN-TARGET: target '{t}' is not defined in omni.yaml");
                return Err(1);
            }
        }
        requested_targets.to_vec()
    };

    if target_names.is_empty() {
        println!("No enabled targets to serve.");
        return Err(0);
    }

    let overrides: BTreeMap<&str, u16> = port_overrides
        .iter()
        .map(|(t, p)| (t.as_str(), *p))
        .collect();

    let mut ports: BTreeMap<String, u16> = BTreeMap::new();
    let mut explicit_assignments: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for t in &target_names {
        let explicit = overrides.get(t.as_str()).copied().or_else(|| {
            loaded
                .file
                .targets
                .get(t)
                .and_then(|c| c.dev.as_ref())
                .and_then(|d| d.port)
        });
        let port = match explicit {
            Some(p) => {
                explicit_assignments.entry(p).or_default().push(t.clone());
                p
            }
            None => omni_adapter_api::pick_free_port().map_err(|e| {
                eprintln!("error: E-DEV-PORT-UNAVAILABLE: could not pick a free port for target '{t}': {e}");
                1
            })?,
        };
        ports.insert(t.clone(), port);
    }

    let conflicts: Vec<(u16, &Vec<String>)> = explicit_assignments
        .iter()
        .filter(|(_, targets)| targets.len() > 1)
        .map(|(port, targets)| (*port, targets))
        .collect();
    if !conflicts.is_empty() {
        for (port, targets) in &conflicts {
            eprintln!(
                "{}",
                Diagnostic::error(
                    "E-DEV-PORT-CONFLICT",
                    format!(
                        "targets {} are all configured to use port {port} -- each dev target needs its own port",
                        targets.join(", ")
                    ),
                )
                .with_suggestion("set a different 'dev.port' in omni.yaml, or pass --port <target>=<port>")
            );
        }
        return Err(1);
    }

    let mut reports = Vec::new();
    let mut handles = Vec::new();

    for t in &target_names {
        println!("==> building target '{t}'");
        let report = orchestrator::build_target(&loaded, t, &components, &adapters, Mode::Compile);
        diag_print::print_all(&report.diagnostics);
        let port = ports[t];

        match report.status {
            TargetStatus::Success => println!("    target '{t}': SUCCESS"),
            other => {
                println!("    target '{t}': {other:?}, skipping dev serving");
                reports.push(report);
                continue;
            }
        }

        let output_dir = root.join(&loaded.file.build.output).join(t);
        let adapter = report
            .adapter
            .as_ref()
            .and_then(|a| adapters.iter().find(|ad| ad.manifest().id == a.id));

        let Some(adapter) = adapter else {
            println!("    target '{t}': built, but its adapter is unavailable for dev serving");
            reports.push(report);
            continue;
        };

        match adapter.dev_serve_plan(&report, &output_dir) {
            Some(Ok(plan)) => {
                let args: Vec<String> = plan
                    .args
                    .iter()
                    .map(|a| a.replace("{port}", &port.to_string()))
                    .collect();
                match ManagedProcess::spawn(
                    t,
                    &plan.executable,
                    &args,
                    &plan.cwd,
                    &plan.ready,
                    plan.ready_timeout,
                ) {
                    Ok(process) => {
                        println!("READY {t} http://127.0.0.1:{port}");
                        handles.push(TargetHandle {
                            target: t.clone(),
                            port,
                            handle: Handle::Process {
                                process,
                                stop_grace: plan.stop_grace,
                            },
                        });
                    }
                    Err(d) => {
                        diag_print::print_diagnostic(&d);
                        println!("    target '{t}': failed to start its dev server");
                    }
                }
            }
            Some(Err(d)) => {
                diag_print::print_diagnostic(&d);
                println!("    target '{t}': built, but could not be started as a dev server");
            }
            None if adapter.manifest().protocol == "in-process/v1" => {
                match http::serve_in_background(output_dir, port) {
                    Ok(server) => {
                        println!("READY {t} http://127.0.0.1:{port}");
                        handles.push(TargetHandle {
                            target: t.clone(),
                            port,
                            handle: Handle::Static(server),
                        });
                    }
                    Err(e) => {
                        println!("    target '{t}': could not start the static dev server: {e}");
                    }
                }
            }
            None => {
                println!(
                    "    target '{t}': built, but its adapter does not support live dev serving"
                );
            }
        }
        reports.push(report);
    }

    if handles.is_empty() {
        eprintln!("\nerror: no targets could be started for 'omni dev'.");
        return Err(1);
    }

    println!("\n{} target(s) serving:", handles.len());
    for h in &handles {
        println!("  {} -> {}", h.target, h.url());
    }

    Ok(DevSession { handles, reports })
}
