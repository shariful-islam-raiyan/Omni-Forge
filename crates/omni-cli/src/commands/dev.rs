use crate::devrun::{self, ShutdownSignal};
use std::path::Path;

/// Parses `NAME=PORT` override strings from `--port`. Invalid entries are
/// reported and treated as a hard error rather than silently ignored.
fn parse_port_overrides(raw: &[String]) -> Result<Vec<(String, u16)>, String> {
    raw.iter()
        .map(|entry| {
            let (name, port) = entry.split_once('=').ok_or_else(|| {
                format!("invalid --port value '{entry}': expected the form NAME=PORT")
            })?;
            let port: u16 = port.parse().map_err(|_| {
                format!("invalid --port value '{entry}': '{port}' is not a valid port number")
            })?;
            Ok((name.to_string(), port))
        })
        .collect()
}

pub fn run(cwd: &Path, targets: &[String], port_overrides: &[String]) -> i32 {
    let overrides = match parse_port_overrides(port_overrides) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    match devrun::start(cwd, targets, &overrides) {
        Ok(session) => session.wait_and_shutdown(ShutdownSignal::CtrlC),
        Err(code) => code,
    }
}
