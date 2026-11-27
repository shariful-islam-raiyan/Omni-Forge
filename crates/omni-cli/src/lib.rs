//! The `omni` CLI as a library, so its command wiring and adapter
//! registration can be exercised directly from integration tests (not only
//! black-box through the compiled binary) — see
//! `docs/decisions/0004-cli-lib-split.md` for why.

pub mod adapters;
pub mod commands;
pub mod devrun;
pub mod diag_print;
pub mod http;
pub mod orchestrator;
pub mod registry;

use clap::{Parser, Subcommand};
use std::env;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "omni",
    version,
    about = "OmniForge: one project spec, multiple build targets."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold a new OmniForge project in a new directory.
    Create { name: String },
    /// Validate the project configuration, components, and resolution for enabled targets.
    Check {
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Inspect available adapters.
    Adapter {
        #[command(subcommand)]
        action: AdapterAction,
    },
    /// Check the local environment and adapter toolchain requirements.
    Doctor,
    /// Build one target (e.g. "web", "backend") or "all" enabled targets.
    Build {
        target: String,
        /// Accept a build where some enabled targets have no available adapter.
        #[arg(long)]
        allow_partial: bool,
    },
    /// Build and serve one or more targets locally, managing any backend
    /// dev server as a real, supervised process. Omit targets to run
    /// every enabled target that supports a dev workflow.
    Dev {
        targets: Vec<String>,
        /// Override a target's dev port: NAME=PORT. Repeatable.
        #[arg(long = "port", value_name = "TARGET=PORT")]
        port: Vec<String>,
    },
    /// Run project tests via adapters that support the 'test' operation.
    Test,
    /// Archive an already-built target's output directory into a single
    /// distributable .zip file (dist/<target>.zip). Run 'omni build
    /// <target>' first.
    Package { target: String },
    /// Remove the project's build output directory.
    Clean,
    /// Real, honest technical-SEO checks against an already-built target's
    /// output. Never a ranking guarantee -- see docs/seo.md.
    Seo {
        #[command(subcommand)]
        action: SeoAction,
    },
}

#[derive(Subcommand)]
pub enum SeoAction {
    /// Run SEO checks and exit non-zero if any fail -- usable as a CI gate.
    Check {
        target: String,
        #[arg(long)]
        json: bool,
    },
    /// Run the same checks, always exiting 0 -- informational only.
    Report {
        target: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum AdapterAction {
    /// List adapters available in this build.
    List {
        #[arg(long)]
        json: bool,
    },
}

/// Parses `argv` and runs the matching command, returning a process exit
/// code. Shared by the real `main()` and anything (tests, future
/// embedders) that wants to drive the CLI without spawning a subprocess.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {e}");
            return ExitCode::from(1);
        }
    };

    let code = match cli.command {
        Command::Create { name } => commands::create::run(&name),
        Command::Check { target, json } => commands::check::run(&cwd, target.as_deref(), json),
        Command::Adapter { action } => match action {
            AdapterAction::List { json } => commands::adapter_list::run(json),
        },
        Command::Doctor => commands::doctor::run(),
        Command::Build {
            target,
            allow_partial,
        } => commands::build::run(&cwd, &target, allow_partial),
        Command::Dev { targets, port } => commands::dev::run(&cwd, &targets, &port),
        Command::Test => commands::test::run(&cwd),
        Command::Package { target } => commands::package::run(&cwd, &target),
        Command::Clean => commands::clean::run(&cwd),
        Command::Seo { action } => match action {
            SeoAction::Check { target, json } => commands::seo::check(&cwd, &target, json),
            SeoAction::Report { target, json } => commands::seo::report(&cwd, &target, json),
        },
    };

    ExitCode::from(code as u8)
}
