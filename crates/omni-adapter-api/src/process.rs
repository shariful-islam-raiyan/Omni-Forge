//! Out-of-process adapter execution: the one mechanism every subprocess
//! adapter in this repository goes through.
//!
//! An adapter implemented as a separate process (today: `python-backend`)
//! is wrapped in a [`ProcessAdapter`], which implements the same [`Adapter`]
//! trait as an in-process adapter — so `omni-cli`'s orchestrator never has
//! to know or care which kind it's calling. `ProcessAdapter` is itself
//! language-neutral: it knows nothing Python-specific. A future Node,
//! Go, or Rust-as-a-subprocess adapter reuses this exact module.
//!
//! Design constraints this module exists to satisfy (see
//! `docs/adapter-protocol.md` and `docs/security-model.md`):
//!
//! - **No shell.** Every spawn is `Command::new(executable).arg(script)` —
//!   an explicit executable and an explicit argument array, never a shell
//!   string.
//! - **Controlled working directory.** The child's cwd is always the
//!   project root the core already validated.
//! - **Controlled environment.** The child does not inherit the parent's
//!   full environment; only a small, fixed allowlist needed for the
//!   interpreter to start at all (`PATH`, a few OS/temp-dir variables) is
//!   passed through. No secrets, no arbitrary parent env.
//! - **Enforced timeout.** The parent polls the child with `try_wait()` and
//!   kills it if it runs past its deadline — a hung or malicious adapter
//!   process cannot block a build indefinitely.
//! - **Protocol discipline.** Exactly one JSON request line goes to stdin;
//!   exactly one JSON response line is read from stdout. Anything else
//!   (missing line, invalid JSON, wrong protocol version, mismatched
//!   request id) is a structured `Diagnostic`, never a panic and never a
//!   silently-assumed success.

use crate::adapter::{Adapter, Artifact, BuildContext, CompileOutput, DevServePlan};
use crate::devserver::ReadyCheck;
use crate::protocol::{
    AdapterRequest, AdapterResponse, Operation, ResponseStatus, PROTOCOL_VERSION,
};
use omni_core::diagnostics::{Diagnostic, Diagnostics};
use omni_core::paths::safe_join;
use omni_core::spec::adapter::AdapterManifest;
use omni_core::spec::build_report::TargetReport;
use omni_core::spec::component::ComponentManifest;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TOOL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const DRAIN_GRACE: Duration = Duration::from_millis(500);
const STDERR_TAIL_MAX: usize = 2000;

/// Tries each candidate executable name in order (no shell; argv only),
/// running `<candidate> --version` with a short timeout, and returns the
/// first one that actually starts and exits successfully. This is how
/// OmniForge tolerates a platform having `python3`, `python`, or (on
/// Windows, where the first two are sometimes a broken Microsoft Store
/// alias stub) the `py` launcher under different names, without ever
/// shelling out to resolve them itself.
pub fn resolve_executable(candidates: &[String]) -> Option<String> {
    candidates.iter().find(|c| probe(c)).cloned()
}

/// Expands a canonical requirement name — the kind of string an adapter
/// manifest declares in `capabilities.requires`, e.g. `"python3"` — into
/// the concrete executable names actually worth trying on this platform,
/// in order. Anything not in this table is tried verbatim (a
/// single-element list of the name itself). This is a convenience table
/// for a handful of common runtimes, not a general package-name resolver.
pub fn known_aliases(canonical: &str) -> Vec<String> {
    let candidates: &[&str] = match canonical {
        // On some Windows installs, `python`/`python3` on PATH are broken
        // Microsoft Store "app execution alias" stubs that exit non-zero
        // immediately rather than running Python; the `py` launcher
        // resolves to a real interpreter in that case.
        "python3" | "python" => &["python3", "python", "py"],
        // Debian/Ubuntu's `nodejs` package historically installed only a
        // `nodejs` binary, not `node`, on PATH (a long-standing packaging
        // naming conflict with an unrelated "node" package); trying both
        // keeps the node-backend adapter working there without guessing.
        "node" => &["node", "nodejs"],
        other => return vec![other.to_string()],
    };
    candidates.iter().map(|s| s.to_string()).collect()
}

/// Strips the Windows "verbatim" (`\\?\...`) path prefix that
/// `std::fs::canonicalize` produces, if present — a no-op on non-Windows
/// paths and on any path not already in that form.
///
/// `omni-core`'s project-root canonicalization (`loader::find_project_root`)
/// is load-bearing for path-safety checks (`safe_join`/`is_within`) and is
/// not changed here. This function only changes how an already-resolved
/// absolute path is *spelled* when handed to a child process's argv/cwd —
/// never which file it refers to. It exists because some external
/// interpreters mishandle the verbatim form as their own main-script
/// argument: observed concretely with Node.js on this project's own
/// development machine, where a canonicalized project root whose directory
/// name contains a space (`D:\Omni Forge\...`) made Node's internal
/// `Module._findPath` → `realpathSync` throw `EISDIR` on a path it
/// mis-parsed down to just `D:` — a real bug, not a hypothetical one.
/// Python, by contrast, handles the verbatim form correctly, which is why
/// this was not already needed for `python-backend`.
pub fn display_for_child(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn probe(candidate: &str) -> bool {
    let mut child = match Command::new(candidate)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if start.elapsed() > TOOL_PROBE_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return false,
        }
    }
}

/// A fixed allowlist of environment variables passed through to adapter
/// child processes. Everything else from the parent's environment —
/// including any secrets — is dropped. This list is only what's needed for
/// a standard language runtime (Python, Node, ...) to start up and find
/// itself on a normal Windows/Linux/macOS install.
///
/// `LocalAppData` was added after a real, reproduced failure (not a
/// hypothetical one): `go build`'s default build cache location on
/// Windows is derived from `os.UserCacheDir()`, which specifically reads
/// `%LocalAppData%` — without it in this allowlist, `go build` refuses to
/// run at all ("GOCACHE is not defined and %LocalAppData% is not
/// defined"), observed building `go-backend`'s first real example on this
/// project's own development machine. General-purpose, not Go-specific —
/// any future tool with the same Windows convention benefits too.
///
/// `pub` so that other process-spawning code which needs the exact same
/// controlled-environment guarantee (today: the test-only backend-server
/// harness in `crates/omni-cli/tests/support/`) reuses this one allowlist
/// instead of maintaining a second copy that could drift.
pub fn controlled_env() -> Vec<(String, String)> {
    [
        "PATH",
        "SystemRoot",
        "SystemDrive",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "LocalAppData",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok().map(|v| (key.to_string(), v)))
    .collect()
}

/// [`controlled_env`] plus a small, explicit set of extra variables a
/// particular tool genuinely needs to run (e.g. `JAVA_HOME`/`ANDROID_HOME`
/// for Gradle) — still never the parent's full environment, only exactly
/// what's passed in on top of the same fixed allowlist every other
/// controlled child process gets.
pub fn controlled_env_with(extra: &[(String, String)]) -> Vec<(String, String)> {
    let mut env = controlled_env();
    env.extend(extra.iter().cloned());
    env
}

fn truncate_tail(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.len() <= max {
        trimmed.to_string()
    } else {
        format!("...{}", &trimmed[trimmed.len() - max..])
    }
}

/// Runs one request/response round trip against an already-spawned-style
/// invocation: `executable script_path`, cwd `cwd`, stdin = `request` as one
/// JSON line, stdout = one JSON line expected back. Enforces `timeout`.
///
/// Returns the parsed, protocol-and-id-validated response, or a single
/// `Diagnostic` describing exactly what went wrong (spawn failure, timeout,
/// non-zero exit with no usable response, empty response, malformed JSON,
/// protocol version mismatch, or request-id mismatch).
pub fn execute(
    executable: &str,
    script_path: &Path,
    cwd: &Path,
    request: &AdapterRequest,
    timeout: Duration,
) -> Result<AdapterResponse, Diagnostic> {
    let request_line = serde_json::to_string(request).map_err(|e| {
        Diagnostic::error(
            "E-ADAPTER-REQUEST-ENCODE",
            format!("failed to encode adapter request: {e}"),
        )
    })?;

    let mut child = Command::new(executable)
        .arg(display_for_child(script_path))
        .current_dir(display_for_child(cwd))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(controlled_env())
        .spawn()
        .map_err(|e| {
            Diagnostic::error(
                "E-ADAPTER-SPAWN-FAILED",
                format!(
                    "failed to start adapter process '{executable} {}': {e}",
                    script_path.display()
                ),
            )
            .with_suggestion("run 'omni doctor' to check adapter toolchain requirements")
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(request_line.as_bytes()).is_ok() {
            let _ = stdin.write_all(b"\n");
        }
        // `stdin` is dropped here, closing the pipe so a well-behaved
        // adapter's readline() returns instead of blocking forever.
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (line_tx, line_rx) = mpsc::channel::<Option<String>>();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap_or(0);
        let _ = line_tx.send(if n == 0 { None } else { Some(line) });
    });

    let (err_tx, err_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf);
        let _ = err_tx.send(buf);
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let stderr_text = err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
                    return Err(Diagnostic::error(
                        "E-ADAPTER-TIMEOUT",
                        format!("adapter process '{executable}' did not respond within {timeout:?} and was killed"),
                    )
                    .with_explanation(truncate_tail(&stderr_text, STDERR_TAIL_MAX)));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(Diagnostic::error(
                    "E-ADAPTER-WAIT-FAILED",
                    format!("failed to wait on adapter process: {e}"),
                ));
            }
        }
    };

    let response_line = line_rx.recv_timeout(DRAIN_GRACE).ok().flatten();
    let stderr_text = err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();

    let Some(line) = response_line else {
        return Err(Diagnostic::error(
            "E-ADAPTER-PROTOCOL-EMPTY-RESPONSE",
            format!("adapter process '{executable}' exited ({status}) without producing a response line on stdout"),
        )
        .with_explanation(truncate_tail(&stderr_text, STDERR_TAIL_MAX)));
    };

    let parsed: AdapterResponse = serde_json::from_str(line.trim()).map_err(|e| {
        Diagnostic::error(
            "E-ADAPTER-PROTOCOL-MALFORMED",
            format!("adapter response was not valid JSON: {e}"),
        )
        .with_explanation(format!("raw line: {}", truncate_tail(&line, 300)))
    })?;

    if parsed.protocol != PROTOCOL_VERSION {
        return Err(Diagnostic::error(
            "E-ADAPTER-PROTOCOL-VERSION",
            format!(
                "adapter responded with protocol '{}', expected '{PROTOCOL_VERSION}'",
                parsed.protocol
            ),
        ));
    }
    if parsed.request_id != request.request_id {
        return Err(Diagnostic::error(
            "E-ADAPTER-PROTOCOL-REQUEST-ID-MISMATCH",
            format!(
                "adapter response request_id '{}' does not match request '{}'",
                parsed.request_id, request.request_id
            ),
        ));
    }

    Ok(parsed)
}

/// Captured result of a [`run_tool`] invocation.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `executable args...` with cwd `cwd` and exactly `env` as the
/// child's environment (no shell, no inherited parent environment beyond
/// what the caller explicitly includes in `env` — see [`controlled_env`]/
/// [`controlled_env_with`] to build a safe baseline), enforcing `timeout`
/// via the same poll-then-kill pattern [`execute`] uses.
///
/// Unlike `execute`, this makes no assumption about the child's output
/// format — it exists for driving a real external build tool that does
/// not speak OmniForge's own adapter wire protocol (today: Gradle, for the
/// `android-gradle` adapter). Both stdout and stderr are captured in full
/// (not just a tail) so a caller can report a build tool's real diagnostic
/// output on failure.
pub fn run_tool(
    executable: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    timeout: Duration,
) -> Result<ToolOutput, Diagnostic> {
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(display_for_child(cwd))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(env.iter().cloned())
        .spawn()
        .map_err(|e| {
            Diagnostic::error(
                "E-TOOL-SPAWN-FAILED",
                format!("failed to start '{executable}': {e}"),
            )
        })?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (out_tx, out_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut reader = stdout;
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf);
        let _ = out_tx.send(buf);
    });
    let (err_tx, err_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = String::new();
        let _ = reader.read_to_string(&mut buf);
        let _ = err_tx.send(buf);
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Diagnostic::error(
                        "E-TOOL-TIMEOUT",
                        format!("'{executable}' did not finish within {timeout:?} and was killed"),
                    ));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                return Err(Diagnostic::error(
                    "E-TOOL-WAIT-FAILED",
                    format!("failed to wait on '{executable}': {e}"),
                ));
            }
        }
    };

    let stdout_text = out_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
    let stderr_text = err_rx.recv_timeout(DRAIN_GRACE).unwrap_or_default();
    Ok(ToolOutput {
        success: status.success(),
        stdout: stdout_text,
        stderr: stderr_text,
    })
}

fn materialize_script(
    project_root: &Path,
    adapter_id: &str,
    filename: &str,
    source: &str,
) -> Result<PathBuf, Diagnostic> {
    for (label, value) in [("adapter id", adapter_id), ("script filename", filename)] {
        if value.contains('/') || value.contains('\\') || value.contains("..") {
            return Err(Diagnostic::error(
                "E-ADAPTER-INVALID-SCRIPT-NAME",
                format!("{label} '{value}' must not contain path separators"),
            ));
        }
    }
    let dir = project_root
        .join(".omniforge")
        .join("adapters")
        .join(adapter_id);
    fs::create_dir_all(&dir).map_err(|e| {
        Diagnostic::error(
            "E-ADAPTER-MATERIALIZE-FAILED",
            format!("could not create adapter working directory: {e}"),
        )
    })?;
    let path = dir.join(filename);
    fs::write(&path, source).map_err(|e| {
        Diagnostic::error(
            "E-ADAPTER-MATERIALIZE-FAILED",
            format!("could not write adapter script: {e}"),
        )
    })?;
    Ok(path)
}

struct RunResult {
    diagnostics: Diagnostics,
    artifacts: Vec<Artifact>,
}

impl RunResult {
    fn failed(d: Diagnostic) -> Self {
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(d);
        Self {
            diagnostics,
            artifacts: Vec::new(),
        }
    }
}

/// Wraps one out-of-process adapter so it can be used anywhere the
/// in-process [`Adapter`] trait is expected. `launcher_candidates` is tried
/// in order (see [`resolve_executable`]); `script_filename`/`script_source`
/// are materialized into `<project_root>/.omniforge/adapters/<id>/` before
/// each run (cheap, always matches the embedded version, and stays inside
/// the already-gitignored `.omniforge/` convention the build cache uses).
pub struct ProcessAdapter {
    manifest: AdapterManifest,
    launcher_candidates: Vec<String>,
    script_filename: String,
    script_source: String,
    timeout: Duration,
}

impl ProcessAdapter {
    pub fn new(
        manifest: AdapterManifest,
        launcher_candidates: Vec<String>,
        script_filename: impl Into<String>,
        script_source: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            manifest,
            launcher_candidates,
            script_filename: script_filename.into(),
            script_source: script_source.into(),
            timeout,
        }
    }

    fn build_request(&self, ctx: &BuildContext, operation: Operation) -> AdapterRequest {
        let request_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );

        let mut components_json = Vec::new();
        let mut source_files = Vec::new();
        for c in &ctx.components {
            let manifest_path = c.manifest_dir.join("component.yaml");
            let interface = fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|raw| serde_yaml::from_str::<ComponentManifest>(&raw).ok())
                .map(|m| m.interface);
            let source_path = safe_join(&c.manifest_dir, &c.implementation.source).ok();
            if let Some(p) = &source_path {
                source_files.push(p.display().to_string());
            }
            components_json.push(serde_json::json!({
                "name": c.name,
                "version": c.version,
                "kind": c.kind.to_string(),
                "interface": interface,
                "source_path": source_path.map(|p| p.display().to_string()),
            }));
        }

        let mut config = BTreeMap::new();
        config.insert(
            "components".to_string(),
            serde_json::Value::Array(components_json),
        );

        AdapterRequest {
            protocol: PROTOCOL_VERSION.to_string(),
            request_id,
            operation,
            project_root: ctx.project_root.display().to_string(),
            target: ctx.target.clone(),
            component: ctx.components.last().map(|c| c.name.clone()),
            source_files,
            entry_path: ctx.entry_path.as_ref().map(|p| p.display().to_string()),
            output_dir: ctx.output.path().display().to_string(),
            config,
            timeout_ms: u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX),
        }
    }

    fn interpret_response(&self, ctx: &BuildContext, response: AdapterResponse) -> RunResult {
        let mut diagnostics = Diagnostics::new();
        for d in response.diagnostics {
            diagnostics.push(d);
        }

        match response.status {
            ResponseStatus::Error => {
                if !diagnostics.has_errors() {
                    diagnostics.push(Diagnostic::error(
                        "E-ADAPTER-REPORTED-ERROR",
                        format!(
                            "adapter '{}' reported status=error without an error-level diagnostic",
                            self.manifest.id
                        ),
                    ));
                }
                RunResult {
                    diagnostics,
                    artifacts: Vec::new(),
                }
            }
            ResponseStatus::Ok => {
                let mut artifacts = Vec::new();
                let mut unsafe_found = false;
                for rel in &response.artifacts {
                    match ctx.output.join(rel) {
                        Ok(absolute_path) => artifacts.push(Artifact {
                            relative_path: rel.clone(),
                            absolute_path,
                        }),
                        Err(_) => {
                            unsafe_found = true;
                            diagnostics.push(Diagnostic::error(
                                "E-ADAPTER-UNSAFE-ARTIFACT-PATH",
                                format!(
                                    "adapter '{}' reported an artifact path outside the output directory: '{rel}'",
                                    self.manifest.id
                                ),
                            ));
                        }
                    }
                }
                if unsafe_found {
                    RunResult {
                        diagnostics,
                        artifacts: Vec::new(),
                    }
                } else {
                    RunResult {
                        diagnostics,
                        artifacts,
                    }
                }
            }
        }
    }

    fn run(&self, ctx: &BuildContext, operation: Operation) -> RunResult {
        let Some(launcher) = resolve_executable(&self.launcher_candidates) else {
            return RunResult::failed(
                Diagnostic::error(
                    "E-ADAPTER-TOOLCHAIN-MISSING",
                    format!(
                        "no usable executable found for adapter '{}' among: {}",
                        self.manifest.id,
                        self.launcher_candidates.join(", ")
                    ),
                )
                .with_target(ctx.target.as_str())
                .with_suggestion("install one of the listed tools and ensure it is on PATH, or run 'omni doctor'"),
            );
        };

        let script_path = match materialize_script(
            &ctx.project_root,
            &self.manifest.id,
            &self.script_filename,
            &self.script_source,
        ) {
            Ok(p) => p,
            Err(d) => return RunResult::failed(d),
        };

        let request = self.build_request(ctx, operation);

        match execute(
            &launcher,
            &script_path,
            &ctx.project_root,
            &request,
            self.timeout,
        ) {
            Err(d) => RunResult::failed(d),
            Ok(response) => self.interpret_response(ctx, response),
        }
    }
}

impl Adapter for ProcessAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn validate(&self, ctx: &BuildContext) -> Diagnostics {
        self.run(ctx, Operation::Validate).diagnostics
    }

    fn compile(&self, ctx: &BuildContext) -> CompileOutput {
        let result = self.run(ctx, Operation::Compile);
        CompileOutput {
            artifacts: result.artifacts,
            diagnostics: result.diagnostics,
        }
    }

    /// Generic across every `ProcessAdapter` (no Python-specific logic):
    /// if the manifest declares `dev`, this build's packaged output must
    /// follow the "runnable-package convention" documented in
    /// `docs/adapter-protocol.md` — exactly one file under `app/` (the
    /// entry script) and a `run-config.json` at the output root — or this
    /// returns a diagnostic rather than silently doing nothing.
    fn dev_serve_plan(
        &self,
        report: &TargetReport,
        output_dir: &Path,
    ) -> Option<Result<DevServePlan, Diagnostic>> {
        let dev = self.manifest.dev.as_ref()?;

        let app_entries: Vec<&str> = report
            .artifacts
            .iter()
            .map(|a| a.path.as_str())
            .filter(|p| {
                p.strip_prefix("app/")
                    .map(|rest| !rest.is_empty() && !rest.contains('/'))
                    .unwrap_or(false)
            })
            .collect();
        let has_config = report.artifacts.iter().any(|a| a.path == "run-config.json");

        if app_entries.len() != 1 || !has_config {
            return Some(Err(Diagnostic::error(
                "E-DEVSERVER-ARTIFACT-LAYOUT",
                format!(
                    "adapter '{}' declares a dev capability but target '{}'s build output does not follow the runnable-package convention (expected exactly one file under 'app/' and a 'run-config.json'; found {} app file(s), run-config.json present: {has_config})",
                    self.manifest.id, report.target, app_entries.len()
                ),
            )
            .with_target(report.target.as_str())));
        }
        let entry = app_entries[0];

        let Some(executable) = resolve_executable(&self.launcher_candidates) else {
            return Some(Err(Diagnostic::error(
                "E-ADAPTER-TOOLCHAIN-MISSING",
                format!(
                    "no usable executable found for adapter '{}' among: {}",
                    self.manifest.id,
                    self.launcher_candidates.join(", ")
                ),
            )
            .with_target(report.target.as_str())
            .with_suggestion(
                "install one of the listed tools and ensure it is on PATH, or run 'omni doctor'",
            )));
        };

        let mut args = vec![
            display_for_child(&output_dir.join(entry))
                .display()
                .to_string(),
            display_for_child(&output_dir.join("run-config.json"))
                .display()
                .to_string(),
        ];
        args.extend(dev.args.iter().cloned());

        Some(Ok(DevServePlan {
            executable,
            args,
            cwd: display_for_child(output_dir),
            ready: ReadyCheck::StdoutLinePrefix(dev.ready_stdout_prefix.clone()),
            ready_timeout: Duration::from_millis(dev.ready_timeout_ms),
            stop_grace: Duration::from_millis(dev.stop_grace_ms),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const PYTHON_CANDIDATES: &[&str] = &["python3", "python", "py"];

    fn candidates() -> Vec<String> {
        PYTHON_CANDIDATES.iter().map(|s| s.to_string()).collect()
    }

    fn write_script(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn sample_request(id: &str) -> AdapterRequest {
        AdapterRequest {
            protocol: PROTOCOL_VERSION.to_string(),
            request_id: id.to_string(),
            operation: Operation::Validate,
            project_root: ".".to_string(),
            target: "backend".to_string(),
            component: None,
            source_files: vec![],
            entry_path: None,
            output_dir: ".".to_string(),
            config: BTreeMap::new(),
            timeout_ms: 1000,
        }
    }

    #[test]
    fn resolve_executable_returns_none_for_bogus_candidates() {
        let bogus = vec!["definitely-not-a-real-executable-xyz123".to_string()];
        assert!(resolve_executable(&bogus).is_none());
    }

    #[test]
    fn resolve_executable_finds_a_working_python() {
        // This asserts the *test environment* has some usable Python —
        // true on this project's dev machine (via the Windows `py`
        // launcher) and on GitHub-hosted ubuntu-latest/windows-latest
        // runners, which both ship Python by default.
        assert!(resolve_executable(&candidates()).is_some());
    }

    #[test]
    fn execute_rejects_non_json_output() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "malformed.py", "print('not json at all')\n");
        let err = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r1"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-PROTOCOL-MALFORMED");
    }

    #[test]
    fn execute_rejects_nonzero_exit_with_no_response() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "nonzero.py", "import sys\nsys.exit(7)\n");
        let err = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r2"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-PROTOCOL-EMPTY-RESPONSE");
    }

    #[test]
    fn execute_times_out_on_a_hanging_process() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "slow.py", "import time\ntime.sleep(60)\n");
        let start = Instant::now();
        let err = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r3"),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-TIMEOUT");
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "should not wait anywhere near the full 60s sleep"
        );
    }

    #[test]
    fn execute_rejects_wrong_protocol_version() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "wrong_protocol.py",
            "import sys, json\nreq = json.loads(sys.stdin.readline())\nprint(json.dumps({'protocol': 'omniforge.dev/adapter-protocol/v0', 'request_id': req['request_id'], 'status': 'ok', 'diagnostics': [], 'artifacts': []}))\n",
        );
        let err = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r4"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-PROTOCOL-VERSION");
    }

    #[test]
    fn execute_rejects_mismatched_request_id() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "wrong_id.py",
            "import sys, json\nsys.stdin.readline()\nprint(json.dumps({'protocol': 'omniforge.dev/adapter-protocol/v1', 'request_id': 'not-the-right-id', 'status': 'ok', 'diagnostics': [], 'artifacts': []}))\n",
        );
        let err = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r5"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-PROTOCOL-REQUEST-ID-MISMATCH");
    }

    #[test]
    fn execute_accepts_a_well_formed_ok_response() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "echo_ok.py",
            "import sys, json\nreq = json.loads(sys.stdin.readline())\nprint(json.dumps({'protocol': 'omniforge.dev/adapter-protocol/v1', 'request_id': req['request_id'], 'status': 'ok', 'diagnostics': [], 'artifacts': []}))\n",
        );
        let response = execute(
            &python,
            &script,
            dir.path(),
            &sample_request("r6"),
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(response.status, ResponseStatus::Ok);
    }

    #[test]
    fn execute_fails_cleanly_for_a_missing_executable() {
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "unused.py", "");
        let err = execute(
            "definitely-not-a-real-executable-xyz123",
            &script,
            dir.path(),
            &sample_request("r7"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-SPAWN-FAILED");
    }

    #[test]
    fn controlled_env_never_leaks_arbitrary_variables() {
        std::env::set_var("OMNIFORGE_TEST_SECRET_SHOULD_NOT_LEAK", "super-secret");
        let env = controlled_env();
        assert!(!env
            .iter()
            .any(|(k, _)| k == "OMNIFORGE_TEST_SECRET_SHOULD_NOT_LEAK"));
        std::env::remove_var("OMNIFORGE_TEST_SECRET_SHOULD_NOT_LEAK");
    }

    #[test]
    fn materialize_script_rejects_path_separators_in_names() {
        let dir = tempdir().unwrap();
        let err = materialize_script(dir.path(), "../escape", "adapter.py", "x").unwrap_err();
        assert_eq!(err.code, "E-ADAPTER-INVALID-SCRIPT-NAME");
    }

    #[test]
    fn controlled_env_with_adds_extra_vars_on_top_of_the_allowlist() {
        let extra = vec![("OMNIFORGE_TEST_EXTRA".to_string(), "value".to_string())];
        let env = controlled_env_with(&extra);
        assert!(env
            .iter()
            .any(|(k, v)| k == "OMNIFORGE_TEST_EXTRA" && v == "value"));
        // The base allowlist is still present underneath.
        assert!(env.iter().any(|(k, _)| k == "PATH"));
    }

    #[test]
    fn run_tool_captures_stdout_and_a_successful_exit() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let args = vec!["-c".to_string(), "print('hello from run_tool')".to_string()];
        let out = run_tool(
            &python,
            &args,
            dir.path(),
            &controlled_env(),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(out.success);
        assert!(out.stdout.contains("hello from run_tool"));
    }

    #[test]
    fn run_tool_reports_a_nonzero_exit_without_treating_it_as_a_spawn_failure() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let args = vec!["-c".to_string(), "import sys; sys.exit(3)".to_string()];
        let out = run_tool(
            &python,
            &args,
            dir.path(),
            &controlled_env(),
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(!out.success);
    }

    #[test]
    fn run_tool_times_out_on_a_hanging_process() {
        let Some(python) = resolve_executable(&candidates()) else {
            return;
        };
        let dir = tempdir().unwrap();
        let args = vec!["-c".to_string(), "import time; time.sleep(60)".to_string()];
        let start = Instant::now();
        let err = run_tool(
            &python,
            &args,
            dir.path(),
            &controlled_env(),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-TOOL-TIMEOUT");
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn run_tool_fails_cleanly_for_a_missing_executable() {
        let dir = tempdir().unwrap();
        let err = run_tool(
            "definitely-not-a-real-executable-xyz123",
            &[],
            dir.path(),
            &controlled_env(),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-TOOL-SPAWN-FAILED");
    }
}
