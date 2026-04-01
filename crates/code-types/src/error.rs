//! Error types used across the entire workspace.
//!
//! Ref: src/utils/errors.ts

use thiserror::Error;

/// Base application error.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("abort: {0}")]
    Abort(String),

    #[error("config parse error in {path}: {message}")]
    ConfigParse { path: String, message: String },

    #[error("shell error (exit {code}): stdout={stdout} stderr={stderr}")]
    Shell {
        stdout: String,
        stderr: String,
        code: i32,
        interrupted: bool,
    },

    #[error("malformed command: {0}")]
    MalformedCommand(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

/// True if the error represents user-initiated or signal-initiated abort.
///
/// Ref: src/utils/errors.ts isAbortError()
pub fn is_abort_error(e: &AppError) -> bool {
    matches!(e, AppError::Abort(_))
}

/// True if an `std::io::Error` means the path is missing or inaccessible.
///
/// Ref: src/utils/errors.ts isFsInaccessible()
pub fn is_fs_inaccessible(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        e.kind(),
        NotFound | PermissionDenied | InvalidInput | NotADirectory
    )
}

/// Shorten a debug-formatted error to at most `max_lines` lines.
///
/// Ref: src/utils/errors.ts shortErrorStack()
pub fn short_error(e: &dyn std::error::Error, max_lines: usize) -> String {
    let full = format!("{e:?}");
    let lines: Vec<&str> = full.lines().collect();
    if lines.len() <= max_lines {
        return full;
    }
    lines[..max_lines].join("\n")
}
