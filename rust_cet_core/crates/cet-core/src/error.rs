//! Error types for the CET engine.
//!
//! Fallible operations return [`CetResult`]. Non-fatal execution issues
//! (truncation, capacity limits) are reported via [`crate::ExecStats`] instead,
//! so callers can inspect partial results.

use thiserror::Error;

/// Errors that can arise from CET operations.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum CetError {
    /// A graph mutation was rejected because a capacity limit was reached.
    #[error("capacity exceeded: {what} (limit = {limit})")]
    CapacityExceeded {
        /// Name of the capacity that was exceeded (e.g. `"vertices"`).
        what: &'static str,
        /// The limit that was hit.
        limit: usize,
    },

    /// An edge referenced a vertex id that was not present in the graph.
    #[error("unknown vertex id: {0}")]
    UnknownVertex(i64),

    /// A vertex id was inserted twice.
    #[error("duplicate vertex id: {0}")]
    DuplicateVertex(i64),

    /// A pattern string was malformed.
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),

    /// A runtime configuration value was out of range.
    #[error("invalid runtime configuration: {0}")]
    InvalidConfig(String),
}

/// Convenience alias for `Result<T, CetError>`.
pub type CetResult<T> = Result<T, CetError>;
