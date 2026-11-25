// Mirrors omni-core's lib.rs: `process::execute` returns `Diagnostic` by
// value (~184 bytes) in its `Err` for ergonomic `?` use at call sites;
// boxing it would add noise with no measurable benefit here.
#![allow(clippy::result_large_err)]

pub mod adapter;
pub mod conformance;
pub mod devserver;
pub mod process;
pub mod protocol;

pub use adapter::{Adapter, Artifact, BuildContext, CompileOutput, DevServePlan};
pub use devserver::{pick_free_port, ManagedProcess, ReadyCheck, StopOutcome};
pub use process::{known_aliases, resolve_executable, ProcessAdapter};
