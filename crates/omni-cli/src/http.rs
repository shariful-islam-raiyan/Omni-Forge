//! A minimal, dependency-free static file server for `omni dev`.
//!
//! Deliberately small: GET only, no TLS, no directory listing, no
//! keep-alive, one thread per connection. This is a local development
//! convenience, not a production server — see docs/getting-started.md.

use omni_core::paths::safe_join;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// A slow/stalled client should not be able to hang this dev server
/// indefinitely by opening a connection and never finishing its request.
const CONNECTION_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the accept loop wakes up to check whether it's been asked to
/// stop. Bounds how long `StaticServer::stop` can take to actually release
/// the port.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A handle to a backgrounded static file server. Dropping this handle
/// does *not* stop the server (matching every other handle type in this
/// codebase being explicit about lifecycle); call [`StaticServer::stop`] to
/// actually release the port.
pub struct StaticServer {
    stop_flag: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl StaticServer {
    /// Signals the accept loop to stop and waits for its thread to exit,
    /// which drops the `TcpListener` and releases the port. Bounded by
    /// `ACCEPT_POLL_INTERVAL` plus whatever a connection already being
    /// handled takes to finish.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Binds `127.0.0.1:port` and serves `root` as static files on a
/// background thread, returning as soon as the bind succeeds (so a port
/// already in use is reported to the caller immediately, not after the
/// fact). The returned [`StaticServer`] must be `.stop()`ped to release the
/// port; it owns no child process, so there is nothing else for `omni
/// dev`'s shutdown path to clean up.
pub fn serve_in_background(root: PathBuf, port: u16) -> std::io::Result<StaticServer> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();
    let thread = thread::spawn(move || {
        loop {
            if flag.load(Ordering::SeqCst) {
                break;
            }
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let root = root.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle(stream, &root) {
                            eprintln!("dev server: connection error: {e}");
                        }
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(e) => {
                    eprintln!("dev server: accept error: {e}");
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
            }
        }
        // `listener` drops here, releasing the port.
    });
    Ok(StaticServer {
        stop_flag,
        thread: Some(thread),
    })
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(CONNECTION_READ_TIMEOUT))?;
    stream.set_write_timeout(Some(CONNECTION_READ_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    // Drain remaining header lines without interpreting them (GET-only, no body expected).
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "GET" {
        return respond(
            &mut stream,
            400,
            "Bad Request",
            b"text/plain",
            b"only GET is supported",
        );
    }

    let raw_path = parts[1];
    let path_only = raw_path.split('?').next().unwrap_or(raw_path);
    let relative = if path_only == "/" {
        "index.html"
    } else {
        path_only.trim_start_matches('/')
    };

    let resolved = match safe_join(root, relative) {
        Ok(p) => p,
        Err(_) => {
            return respond(
                &mut stream,
                400,
                "Bad Request",
                b"text/plain",
                b"invalid path",
            )
        }
    };

    if !resolved.is_file() {
        return respond(&mut stream, 404, "Not Found", b"text/plain", b"not found");
    }

    let mut file = fs::File::open(&resolved)?;
    let mut body = Vec::new();
    file.read_to_end(&mut body)?;
    let content_type = guess_content_type(&resolved);
    respond(&mut stream, 200, "OK", content_type, &body)
}

fn guess_content_type(path: &Path) -> &'static [u8] {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => b"text/html; charset=utf-8",
        "css" => b"text/css; charset=utf-8",
        "js" => b"text/javascript; charset=utf-8",
        "json" => b"application/json",
        "svg" => b"image/svg+xml",
        "png" => b"image/png",
        "ico" => b"image/x-icon",
        _ => b"application/octet-stream",
    }
}

fn respond(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &[u8],
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
        body.len(),
        String::from_utf8_lossy(content_type)
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
