use crate::registry;
use omni_adapter_api::process::{known_aliases, resolve_executable};
use std::process::Command;
use std::time::Duration;

fn check_tool(bin: &str, arg: &str) -> Result<String, String> {
    let mut child = Command::new(bin)
        .arg(arg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().map_err(|e| e.to_string())?;
                if status.success() {
                    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    return Ok(if text.is_empty() {
                        String::from_utf8_lossy(&output.stderr).trim().to_string()
                    } else {
                        text
                    });
                }
                return Err(format!("exited with {status}"));
            }
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(5) {
                    let _ = child.kill();
                    return Err("timed out after 5s".to_string());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Checks a declared `capabilities.requires` entry (e.g. `"python3"`),
/// trying every platform alias `known_aliases` knows about (so a Windows
/// box where the plain `python3`/`python` names are broken Microsoft Store
/// alias stubs, but `py` works, is reported as OK, not MISSING) before
/// falling back to the literal name.
fn check_requirement(canonical: &str) -> Result<String, String> {
    let candidates = known_aliases(canonical);
    let Some(resolved) = resolve_executable(&candidates) else {
        return Err(format!("tried: {}", candidates.join(", ")));
    };
    let version = check_tool(&resolved, "--version")?;
    if resolved == canonical {
        Ok(version)
    } else {
        Ok(format!("{version}, via '{resolved}'"))
    }
}

pub fn run() -> i32 {
    println!("OmniForge environment check\n");

    println!("CLI: omni {}", env!("CARGO_PKG_VERSION"));

    match check_tool("rustc", "--version") {
        Ok(v) => println!("Rust toolchain: OK ({v})"),
        Err(e) => println!("Rust toolchain: NOT FOUND ({e})"),
    }

    println!("\nAdapters:");
    let mut all_ok = true;
    for adapter in registry::built_in_adapters() {
        let m = adapter.manifest();
        if m.capabilities.requires.is_empty() {
            println!(
                "  {} {} — OK (in-process, no external toolchain required)",
                m.id, m.version
            );
            continue;
        }
        for tool in &m.capabilities.requires {
            match check_requirement(tool) {
                Ok(v) => println!("  {} {} — requires '{tool}': OK ({v})", m.id, m.version),
                Err(e) => {
                    println!(
                        "  {} {} — requires '{tool}': MISSING ({e})",
                        m.id, m.version
                    );
                    all_ok = false;
                }
            }
        }
    }

    println!();
    if all_ok {
        println!("All checks passed.");
        0
    } else {
        println!("Some required tools are missing. Builds using the affected adapters will fail.");
        1
    }
}
