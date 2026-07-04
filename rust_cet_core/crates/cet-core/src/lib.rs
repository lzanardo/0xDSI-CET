//! # cet-core
//!
//! Complex Event Temporal (CET) pattern-match engine.
//!
//! This crate provides the core types (`Graph`, `Query`, `Vertex`, `Edge`) and the
//! three execution strategies used by the engine:
//!
//! - **MCET** — depth-first match traversal.
//! - **TCET** — breadth-first temporal traversal.
//! - **HCET** — hybrid: BFS up to a switch depth, then DFS.
//!
//! The C reference implementation lives in `../../../c_engine`. This crate is the
//! Rust port; the migration is behavior-preserving and validated by property tests
//! that assert MCET/TCET/HCET produce identical path sets on any valid input.
//!
//! ## Invariants
//!
//! - Every emitted path is temporally monotonic in `event_time_ms`.
//! - `stats.paths_emitted == out.paths.len()` on non-truncated runs.
//! - Truncation always sets `stats.overflow = true` and records an error message.
//!
//! ## Status
//!
//! Scaffold: types and stubs only. Execution algorithms are `todo!()` pending
//! the port-and-property-test phase.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod exec;
pub mod graph;
pub mod optimizer;
pub mod query;
pub mod result;
pub mod sliding;
pub mod stats;

pub(crate) mod adjacency;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use error::{CetError, CetResult};
pub use graph::{Edge, Graph, Vertex, VertexId};
pub use query::{EventType, Predicate, Query};
pub use result::MatchResult;
pub use stats::ExecStats;

/// Compile-time capacity constants mirroring the C engine.
///
/// In Rust these are used as *defaults* and truncation thresholds, not as
/// fixed array bounds. Collections grow dynamically until the cap is reached.
pub mod caps {
    /// Maximum number of events per pattern sequence.
    pub const MAX_SEQ: usize = 16;
    /// Default cap on paths emitted per query.
    pub const MAX_PATHS: usize = 100_000;
    /// Default cap on path length (number of vertices in a match).
    pub const MAX_PATH_LEN: usize = 64;
    /// Default cap on graphlets tracked by the optimizer.
    pub const MAX_GRAPHLETS: usize = 4096;
    /// Maximum length of an error string carried in stats.
    pub const MAX_ERROR_LEN: usize = 256;
    /// Maximum native worker threads for the parallel executor.
    pub const MAX_NATIVE_THREADS: usize = 64;
}
