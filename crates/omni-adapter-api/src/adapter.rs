//! The in-process adapter trait actually used by `omni-cli` today.

use crate::devserver::ReadyCheck;
use omni_core::diagnostics::{Diagnostic, Diagnostics};
use omni_core::paths::OutputDir;
use omni_core::resolver::ResolvedComponent;
use omni_core::spec::adapter::AdapterManifest;
use omni_core::spec::build_report::TargetReport;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Everything an adapter needs to perform one target's build, already
/// resolved and validated by `omni-core`.
pub struct BuildContext {
    pub project_root: PathBuf,
    pub target: String,
    /// The target's `entry` file (e.g. `web/app.yaml`), resolved and
    /// verified to exist, if the target config declared one.
    pub entry_path: Option<PathBuf>,
    pub output: OutputDir,
    pub components: Vec<ResolvedComponent>,
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub relative_path: String,
    pub absolute_path: PathBuf,
}

pub struct CompileOutput {
    pub artifacts: Vec<Artifact>,
    pub diagnostics: Diagnostics,
}

/// Everything `omni dev` needs to start one target's build output as a
/// long-running dev server. Returned by [`Adapter::dev_serve_plan`].
pub struct DevServePlan {
    pub executable: String,
    /// May contain the literal placeholder `"{port}"`; the caller
    /// substitutes it once a concrete port has been chosen (the plan
    /// itself is built before a port is known).
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub ready: ReadyCheck,
    pub ready_timeout: Duration,
    pub stop_grace: Duration,
}

/// An adapter connects OmniForge to one real toolchain/target. Every
/// operation an adapter does NOT support must be absent from its declared
/// capabilities (`manifest().capabilities`) — the core checks capabilities
/// before calling into any of these, but implementations must still fail
/// loudly rather than silently no-op if called out of turn.
pub trait Adapter {
    fn manifest(&self) -> &AdapterManifest;

    /// Validates that `ctx` is something this adapter can actually build
    /// (entry file parses, referenced components/assets exist, inputs match
    /// declared interfaces). Must not write any files.
    fn validate(&self, ctx: &BuildContext) -> Diagnostics;

    /// Performs the real build. Must only write inside `ctx.output`. Must
    /// return `Err`/error-diagnostics rather than a successful-looking empty
    /// result if the build could not be completed.
    fn compile(&self, ctx: &BuildContext) -> CompileOutput;

    /// Returns a plan for starting `report`'s build output (already
    /// written under `output_dir`) as a long-running dev server, for an
    /// adapter whose manifest declares a `dev` capability.
    ///
    /// - `None` means this adapter has no dev-serve capability at all
    ///   (the default — e.g. in-process adapters like `web-static`, whose
    ///   "dev serving" is the CLI's own static file server, not a child
    ///   process this trait manages).
    /// - `Some(Err(_))` means `dev` *was* declared but this specific build's
    ///   output doesn't satisfy the convention a plan requires (wrong
    ///   artifact layout, no resolvable launcher, ...) — surfaced as a real
    ///   diagnostic, never silently treated as "not supported".
    /// - `Some(Ok(plan))` is a ready-to-spawn plan.
    fn dev_serve_plan(
        &self,
        _report: &TargetReport,
        _output_dir: &Path,
    ) -> Option<Result<DevServePlan, Diagnostic>> {
        None
    }
}
