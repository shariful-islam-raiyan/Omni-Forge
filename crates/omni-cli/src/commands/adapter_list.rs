use crate::registry;

pub fn run(json: bool) -> i32 {
    let adapters = registry::built_in_adapters();

    if json {
        let payload: Vec<_> = adapters.iter().map(|a| a.manifest()).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
        return 0;
    }

    println!(
        "{:<16} {:<10} {:<18} {:<10} OPERATIONS",
        "ID", "VERSION", "PROTOCOL", "TARGETS"
    );
    for a in &adapters {
        let m = a.manifest();
        println!(
            "{:<16} {:<10} {:<18} {:<10} {}",
            m.id,
            m.version,
            m.protocol,
            m.capabilities.targets.join(","),
            m.capabilities.operations.join(",")
        );
        if m.capabilities.requires.is_empty() {
            println!("                 requires: (none — no external toolchain needed)");
        } else {
            println!(
                "                 requires: {}",
                m.capabilities.requires.join(", ")
            );
        }
    }
    let in_process = adapters
        .iter()
        .filter(|a| a.manifest().protocol == "in-process/v1")
        .count();
    let out_of_process = adapters.len() - in_process;
    println!(
        "\n{} adapter(s) available: {in_process} in-process, {out_of_process} out-of-process. All are compiled into this binary — none are discovered on disk or downloaded.",
        adapters.len()
    );
    0
}
