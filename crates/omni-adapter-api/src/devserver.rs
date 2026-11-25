//! Production process-lifecycle management for long-running dev servers
//! (e.g. the real HTTP process a `backend`-target adapter starts for `omni
//! dev`).
//!
//! This generalizes a pattern that already existed, proven and tested, but
//! only as test-only code (`crates/omni-cli/tests/support/mod.rs`, see
//! `docs/decisions/0007-backend-dev-server-is-example-scoped.md` for why it
//! started that way): spawn with no shell and a controlled environment,
//! block until a readiness signal appears on stdout or a timeout elapses,
//! and guarantee the child is killed and reaped even on a panicking
//! caller. What's new here relative to that test harness is **graceful**
//! shutdown: a real attempt at a clean stop (SIGTERM on Unix,
//! `CTRL_BREAK_EVENT` on Windows) before falling back to a hard kill, so a
//! dev server gets a chance to close sockets/flush output instead of being
//! unconditionally `TerminateProcess`/`SIGKILL`'d the instant the user hits
//! Ctrl-C.
//!
//! Windows note: `GenerateConsoleCtrlEvent` only targets the right process
//! (not every process sharing the console, which would include this CLI
//! itself) when the child was spawned with `CREATE_NEW_PROCESS_GROUP` —
//! `spawn` always sets that flag on Windows. The FFI declaration below is
//! two `u32`s and an `i32` return from `kernel32.dll`; it is pure `std`/
//! `extern "system"`, deliberately avoiding a `windows-sys` dependency for
//! something this small.

use crate::process::controlled_env;
use omni_core::diagnostics::Diagnostic;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const STDERR_DRAIN_GRACE: Duration = Duration::from_millis(500);

#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CTRL_BREAK_EVENT: u32 = 1;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GenerateConsoleCtrlEvent(dw_ctrl_event: u32, dw_process_group_id: u32) -> i32;
}

/// How a spawned dev server signals that it is ready to accept connections.
#[derive(Debug, Clone)]
pub enum ReadyCheck {
    /// The process printed one line to stdout whose trimmed start matches
    /// this prefix (e.g. `"LISTENING"`).
    StdoutLinePrefix(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// Exited in response to the graceful signal within the grace period.
    Graceful,
    /// Did not respond in time; a hard kill was used instead.
    HardKilled,
    /// Was already not running when `stop` was called.
    AlreadyExited,
}

/// A supervised, long-running child process. Dropping it always kills and
/// reaps the child if it is still alive — the same safety net
/// `crates/omni-cli/tests/support/mod.rs::ServerGuard` already relied on,
/// now available to production code too.
pub struct ManagedProcess {
    label: String,
    child: Option<Child>,
}

impl std::fmt::Debug for ManagedProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedProcess")
            .field("label", &self.label)
            .finish()
    }
}

impl ManagedProcess {
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Non-blocking check for an unexpected exit. `Some(description)` if the
    /// process has already exited; `None` if it's (as far as we can tell
    /// without blocking) still running.
    pub fn poll_exit(&mut self) -> Option<String> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            _ => None,
        }
    }

    /// Spawns `exe args...` with no shell, `cwd` as the working directory,
    /// and the same fixed environment allowlist every other process-
    /// spawning code path in this crate uses (`controlled_env`). Blocks
    /// until `ready` is satisfied, the process exits early, or
    /// `ready_timeout` elapses — whichever happens first — never hanging
    /// past that bound.
    pub fn spawn(
        label: &str,
        exe: &str,
        args: &[String],
        cwd: &Path,
        ready: &ReadyCheck,
        ready_timeout: Duration,
    ) -> Result<Self, Diagnostic> {
        let mut command = Command::new(exe);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(controlled_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let mut child = command.spawn().map_err(|e| {
            Diagnostic::error(
                "E-DEVSERVER-SPAWN-FAILED",
                format!("failed to start '{label}' dev server ('{exe}'): {e}"),
            )
            .with_target(label)
        })?;

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

        let ReadyCheck::StdoutLinePrefix(prefix) = ready;

        let start = Instant::now();
        loop {
            if let Ok(line) = line_rx.try_recv() {
                return match line {
                    Some(line) if line.trim_start().starts_with(prefix.as_str()) => {
                        Ok(ManagedProcess {
                            label: label.to_string(),
                            child: Some(child),
                        })
                    }
                    Some(other) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        Err(Diagnostic::error(
                            "E-DEVSERVER-UNEXPECTED-OUTPUT",
                            format!(
                                "'{label}' dev server printed an unexpected readiness line: {other:?}"
                            ),
                        )
                        .with_target(label))
                    }
                    None => {
                        let _ = child.wait();
                        let stderr_text =
                            err_rx.recv_timeout(STDERR_DRAIN_GRACE).unwrap_or_default();
                        Err(Diagnostic::error(
                            "E-DEVSERVER-EXITED-BEFORE-READY",
                            format!("'{label}' dev server exited before becoming ready"),
                        )
                        .with_target(label)
                        .with_explanation(stderr_text))
                    }
                };
            }
            if let Ok(Some(status)) = child.try_wait() {
                let stderr_text = err_rx.recv_timeout(STDERR_DRAIN_GRACE).unwrap_or_default();
                return Err(Diagnostic::error(
                    "E-DEVSERVER-EXITED-EARLY",
                    format!("'{label}' dev server exited early with {status}"),
                )
                .with_target(label)
                .with_explanation(stderr_text));
            }
            if start.elapsed() > ready_timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Diagnostic::error(
                    "E-DEVSERVER-READY-TIMEOUT",
                    format!(
                        "'{label}' dev server did not report readiness within {ready_timeout:?}"
                    ),
                )
                .with_target(label));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Attempts a graceful stop (`SIGTERM` on Unix, `CTRL_BREAK_EVENT` on
    /// Windows), waits up to `grace` for the process to exit on its own,
    /// then falls back to a hard kill (`SIGKILL`/`TerminateProcess`) if it
    /// hasn't. Never blocks past `grace` plus the time a hard kill takes.
    pub fn stop(&mut self, grace: Duration) -> StopOutcome {
        let Some(child) = self.child.as_mut() else {
            return StopOutcome::AlreadyExited;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            return StopOutcome::AlreadyExited;
        }

        send_graceful_signal(child.id());

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return StopOutcome::Graceful,
                Ok(None) => {
                    if start.elapsed() > grace {
                        let _ = child.kill();
                        let _ = child.wait();
                        return StopOutcome::HardKilled;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => return StopOutcome::HardKilled,
            }
        }
    }
}

#[cfg(unix)]
fn send_graceful_signal(pid: u32) {
    // SAFETY: `kill` with a valid pid and a standard signal number has no
    // preconditions beyond the pid being a real process id, which it is
    // here (it came from `Child::id()` of a child we are still holding).
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(windows)]
fn send_graceful_signal(pid: u32) {
    // SAFETY: FFI call with two plain integer arguments and no pointers;
    // failure (e.g. no console, or the pid's group doesn't match) is
    // reported via the return value, which the caller tolerates by falling
    // back to a hard kill after `grace` regardless.
    unsafe {
        GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

/// Binds an OS-assigned ephemeral port on 127.0.0.1 and immediately drops
/// the listener so the caller can hand the port number to a child process.
/// Has a small, documented TOCTOU window (something else could grab the
/// port before the child binds it) — acceptable for local dev tooling, not
/// something this function tries to eliminate.
pub fn pick_free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn python_exe() -> Option<String> {
        crate::process::resolve_executable(&crate::process::known_aliases("python3"))
    }

    fn write_script(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn spawn_detects_readiness_line() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "ready.py",
            "import time\nprint('LISTENING 1234', flush=True)\ntime.sleep(5)\n",
        );
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");
        assert!(proc.poll_exit().is_none());
        proc.stop(Duration::from_secs(2));
    }

    #[test]
    fn spawn_times_out_when_never_ready() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "never.py", "import time\ntime.sleep(30)\n");
        let start = Instant::now();
        let err = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-DEVSERVER-READY-TIMEOUT");
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn spawn_reports_early_exit() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "fails.py", "import sys\nsys.exit(2)\n");
        let err = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.code, "E-DEVSERVER-EXITED-BEFORE-READY");
    }

    // On Unix, a process that installs SIG_IGN for SIGTERM truly ignores
    // our graceful signal, so the hard-kill fallback is what terminates it.
    #[cfg(unix)]
    #[test]
    fn stop_hard_kills_a_process_that_ignores_the_graceful_signal() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "stubborn.py",
            "import signal, time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\nprint('LISTENING 1', flush=True)\ntime.sleep(30)\n",
        );
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");

        let start = Instant::now();
        let outcome = proc.stop(Duration::from_millis(300));
        assert_eq!(outcome, StopOutcome::HardKilled);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    // On Windows, a plain `time.sleep()` loop with no console control
    // handler is actually terminated by CTRL_BREAK_EVENT's own default
    // handling (observed empirically: `stop` reports `Graceful`, because
    // the process exits on its own within the grace window) -- so a
    // meaningful "ignores the graceful signal" fixture on Windows must
    // install a real console control handler that swallows the event
    // (returns nonzero / handled) via `ctypes`, forcing our hard-kill
    // fallback to be the thing that actually ends it.
    #[cfg(windows)]
    #[test]
    fn stop_hard_kills_a_process_that_ignores_the_graceful_signal() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "stubborn.py",
            "import ctypes, time\nHANDLER_TYPE = ctypes.WINFUNCTYPE(ctypes.c_int, ctypes.c_uint)\ndef handler(_ctrl_type):\n    return 1\n_h = HANDLER_TYPE(handler)\nctypes.windll.kernel32.SetConsoleCtrlHandler(_h, True)\nprint('LISTENING 1', flush=True)\ntime.sleep(30)\n",
        );
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");

        let start = Instant::now();
        let outcome = proc.stop(Duration::from_millis(300));
        assert_eq!(outcome, StopOutcome::HardKilled);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    /// Regardless of platform or which path `stop` takes internally, the
    /// real guarantee this milestone cares about is a *bounded* shutdown —
    /// this test does not assert which `StopOutcome` occurs.
    #[test]
    fn stop_always_terminates_within_the_bound_whichever_path_is_taken() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "plain_sleep.py",
            "import time\nprint('LISTENING 1', flush=True)\ntime.sleep(30)\n",
        );
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");

        let start = Instant::now();
        proc.stop(Duration::from_millis(500));
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(proc.poll_exit().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn stop_is_graceful_when_the_process_cooperates() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(
            dir.path(),
            "cooperative.py",
            "import signal, sys, time\nsignal.signal(signal.SIGTERM, lambda *_: sys.exit(0))\nprint('LISTENING 1', flush=True)\ntime.sleep(30)\n",
        );
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");

        let outcome = proc.stop(Duration::from_secs(3));
        assert_eq!(outcome, StopOutcome::Graceful);
    }

    #[test]
    fn stop_on_already_exited_process_is_a_no_op() {
        let Some(python) = python_exe() else {
            return;
        };
        let dir = tempdir().unwrap();
        let script = write_script(dir.path(), "quick.py", "print('LISTENING 1', flush=True)\n");
        let mut proc = ManagedProcess::spawn(
            "test",
            &python,
            &[script.display().to_string()],
            dir.path(),
            &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
            Duration::from_secs(5),
        )
        .expect("spawns and becomes ready");

        // Give the (already-exited-by-now) script time to actually exit.
        thread::sleep(Duration::from_millis(200));
        let outcome = proc.stop(Duration::from_secs(2));
        assert_eq!(outcome, StopOutcome::AlreadyExited);
    }

    #[test]
    fn pick_free_port_returns_a_bindable_port() {
        let port = pick_free_port().expect("pick a port");
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
    }
}
