use omni_core::loader;
use omni_core::paths::OutputDir;
use std::path::Path;

pub fn run(cwd: &Path) -> i32 {
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

    let output_rel = &loaded.file.build.output;
    let output = match OutputDir::resolve(&root, output_rel) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    match output.remove_all(&root) {
        Ok(()) => {
            println!("Removed '{}'.", output.path().display());
            0
        }
        Err(e) => {
            eprintln!("error: refused to clean: {e}");
            1
        }
    }
}
