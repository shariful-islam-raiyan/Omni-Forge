use crate::diagnostics::Diagnostic;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OmniError {
    #[error("{0}")]
    Diagnostic(Diagnostic),

    #[error("validation failed with {} error(s)", .0.len())]
    Invalid(Vec<Diagnostic>),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("path '{path}' escapes the permitted root '{root}'")]
    UnsafePath { path: String, root: String },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, OmniError>;

impl From<Diagnostic> for OmniError {
    fn from(d: Diagnostic) -> Self {
        OmniError::Diagnostic(d)
    }
}
