//! Test-only HTTP helpers for the `hello-fullstack` end-to-end tests
//! (`tests/fullstack_http.rs`), plus a thin [`ServerGuard`] wrapper that now
//! delegates real process lifecycle management to the production
//! `omni_adapter_api::devserver` module (promoted there from this exact file
//! during the "Unified Application Runtime" milestone — see
//! `docs/decisions/0009-unified-dev-process-lifecycle.md`). This module
//! keeps its own HTTP client and `ensure_within` path check, which stay
//! test-only (`fullstack_http.rs` needs a hand-rolled HTTP/1.1 client that
//! has no production use).
//!
//! Safety properties this module upholds, matching
//! `crates/omni-adapter-api/src/process.rs`/`devserver.rs`'s existing rules
//! for the real adapter mechanism:
//! - **No shell**, **controlled environment**, **bounded waits**, and
//!   **cleanup-on-drop** are all inherited from `devserver::ManagedProcess`.
//! - **Path safety** — the script/config paths are checked to resolve
//!   inside the project's own build output before spawning anything.
//!
//! This file is `include!`d (via `mod support;`) separately into every
//! test binary under `tests/` that needs it; not every binary uses every
//! helper (e.g. `dev_lifecycle.rs` only needs `copy_dir_all`/`http_request`/
//! `pick_free_port`, not the `spawn_backend_server`/`ServerGuard` pair that
//! `fullstack_http.rs` needs), so per-binary dead-code warnings here would
//! be noise rather than a real signal.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use omni_adapter_api::devserver::{self, ReadyCheck};

/// Recursively copies `src` into `dst` (which must already exist). Used to
/// give each test its own private copy of `examples/hello-fullstack` so
/// concurrently-running tests never race on the same `dist/` output.
///
/// Deliberately skips `dist/` and `.omniforge/` (build output and the local
/// build cache — both gitignored, never part of "the project"): if the real
/// `examples/hello-fullstack` on disk has ever actually been built (e.g. by
/// running `omni build` or `scripts/verify.sh` by hand), copying those
/// directories verbatim would hand a "fresh" test its own real, stale cache
/// entries, silently turning an expected cache miss into a hit. This was a
/// real, reproducible bug (not flakiness): `editing_the_contract_busts_the_
/// backend_build_cache` fails deterministically once `examples/hello-
/// fullstack/.omniforge/cache` exists on disk, for exactly this reason.
pub fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && matches!(entry.file_name().to_str(), Some("dist" | ".omniforge")) {
            continue;
        }
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_all(&entry.path(), &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Binds an OS-assigned ephemeral port on 127.0.0.1 and immediately drops
/// the listener so the caller can hand the port number to a child process.
/// This has a small, documented TOCTOU window (something else could grab
/// the port before the child binds it) — acceptable for a local test
/// harness, not something this module tries to eliminate. Delegates to the
/// production helper (`omni_adapter_api::devserver::pick_free_port`).
pub fn pick_free_port() -> u16 {
    devserver::pick_free_port().expect("bind an ephemeral port")
}

/// Verifies `path` resolves (after canonicalization) inside `root`
/// (also canonicalized), returning the canonical path on success. Defense
/// in depth: in practice both paths here are always this test's own build
/// output, never user input, but the harness is reviewed as process-
/// spawning code so it gets the same containment check production
/// path-handling code uses (`omni_core::paths`).
fn ensure_within(path: &Path, root: &Path) -> Result<std::path::PathBuf, String> {
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|e| format!("could not canonicalize '{}': {e}", path.display()))?;
    let canonical_root = std::fs::canonicalize(root)
        .map_err(|e| format!("could not canonicalize '{}': {e}", root.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!(
            "'{}' does not resolve inside '{}'",
            path.display(),
            root.display()
        ));
    }
    Ok(canonical_path)
}

/// RAII guard around a spawned backend server child process. Thin wrapper
/// around `omni_adapter_api::devserver::ManagedProcess`, which does the
/// actual kill-and-reap-on-drop (including on a panicking test). Every
/// caller in this test suite only needs the drop side effect (keeping the
/// guard alive for the test's duration), never the field directly.
#[derive(Debug)]
pub struct ServerGuard {
    #[allow(dead_code)]
    process: devserver::ManagedProcess,
}

/// Spawns `<python_exe> <script_path> <config_path> --serve <port>` with no
/// shell, `cwd` as the working directory, and the same controlled
/// environment allowlist the real adapter mechanism uses. Blocks until the
/// server prints its `LISTENING <port>` readiness line on stdout, the
/// process exits early, or `ready_timeout` elapses — whichever happens
/// first — and returns a named error for the latter two instead of ever
/// hanging. Delegates to `omni_adapter_api::devserver::ManagedProcess::spawn`.
pub fn spawn_backend_server(
    python_exe: &str,
    script_path: &Path,
    config_path: &Path,
    cwd: &Path,
    port: u16,
    ready_timeout: Duration,
) -> Result<ServerGuard, String> {
    ensure_within(script_path, cwd)?;
    ensure_within(config_path, cwd)?;

    let args = vec![
        script_path.display().to_string(),
        config_path.display().to_string(),
        "--serve".to_string(),
        port.to_string(),
    ];
    devserver::ManagedProcess::spawn(
        "backend",
        python_exe,
        &args,
        cwd,
        &ReadyCheck::StdoutLinePrefix("LISTENING".to_string()),
        ready_timeout,
    )
    .map(|process| ServerGuard { process })
    .map_err(|d| match &d.explanation {
        Some(explanation) if !explanation.is_empty() => {
            format!("{}; stderr: {explanation}", d.message)
        }
        _ => d.message,
    })
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// A hand-rolled, minimal HTTP/1.1 client: writes one request over a
/// `TcpStream`, reads back a status line, headers, and a `Content-Length`-
/// bounded body, with a read timeout so a server that accepts the
/// connection but never responds fails the caller quickly instead of
/// hanging it. Intentionally does not support redirects, chunked transfer
/// encoding, or TLS — this only ever talks to a local test server over
/// plain HTTP, and the server side here never sends a chunked response.
pub fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    read_timeout: Duration,
) -> std::io::Result<HttpResponse> {
    let mut stream = TcpStream::connect((host, port))?;
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_write_timeout(Some(read_timeout))?;

    let body_bytes = body.unwrap_or("").as_bytes();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body_bytes.len()
    );
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    stream.write_all(body_bytes)?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line)?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("could not parse HTTP status line: {status_line:?}"),
            )
        })?;

    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        reader.read_line(&mut header_line)?;
        if header_line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header_line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body_buf = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body_buf)?;
    }
    let body = String::from_utf8_lossy(&body_buf).into_owned();

    Ok(HttpResponse { status, body })
}
