use std::fs;
use std::path::Path;

const TPL_OMNI_YAML: &str = include_str!("../../../../templates/basic/omni.yaml");
const TPL_COMPONENT_YAML: &str =
    include_str!("../../../../templates/basic/lib/components/ui.home/component.yaml");
const TPL_HOME_TMPL: &str =
    include_str!("../../../../templates/basic/lib/components/ui.home/home.html.tmpl");
const TPL_APP_YAML: &str = include_str!("../../../../templates/basic/web/app.yaml");
const TPL_STYLE_CSS: &str = include_str!("../../../../templates/basic/public/style.css");

fn valid_project_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if name.len() < 2 || name.len() > 64 {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn run(name: &str) -> i32 {
    if !valid_project_name(name) {
        eprintln!(
            "error: E-CREATE-INVALID-NAME: '{name}' is not a valid project name.\n  suggestion: use lowercase letters, digits, and hyphens, starting with a letter (e.g. 'hello-web')"
        );
        return 1;
    }

    let target_dir = Path::new(name);
    if target_dir.exists() {
        let has_entries = fs::read_dir(target_dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(true);
        if has_entries {
            eprintln!(
                "error: E-CREATE-DIR-NOT-EMPTY: directory '{name}' already exists and is not empty.\n  suggestion: choose a different name, or run this command in an empty directory"
            );
            return 1;
        }
    }

    let write = |rel: &str, contents: String| -> std::io::Result<()> {
        let path = target_dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    };

    let result = (|| -> std::io::Result<()> {
        write("omni.yaml", TPL_OMNI_YAML.replace("__PROJECT_NAME__", name))?;
        write(
            "lib/components/ui.home/component.yaml",
            TPL_COMPONENT_YAML.to_string(),
        )?;
        write(
            "lib/components/ui.home/home.html.tmpl",
            TPL_HOME_TMPL.to_string(),
        )?;
        write(
            "web/app.yaml",
            TPL_APP_YAML.replace("__PROJECT_NAME__", name),
        )?;
        write("public/style.css", TPL_STYLE_CSS.to_string())?;
        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("error: E-CREATE-IO: failed to write project files: {e}");
        return 1;
    }

    println!("Created new OmniForge project '{name}'.");
    println!();
    println!("Next steps:");
    println!("  cd {name}");
    println!("  omni check");
    println!("  omni build web");
    println!("  omni dev web");
    0
}
