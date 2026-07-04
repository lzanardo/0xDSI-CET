//! Match result type.
//!
//! In Rust we replace the C engine's fixed-size `int paths[CET_MAX_PATHS][CET_MAX_PATH_LEN]`
//! with a `Vec<Vec<VertexId>>`. Truncation is enforced at emit time via the
//! `max_paths` and `max_path_len` fields — the same guarantees the C code provides,
//! but with dynamic allocation and no ~25 MiB stack-hostile struct.

use crate::graph::VertexId;

/// Set of matched paths produced by a query execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchResult {
    /// Emitted paths, each a sequence of vertex ids.
    pub paths: Vec<Vec<VertexId>>,
    /// Cap on the number of paths that may be emitted before truncation.
    pub max_paths: usize,
    /// Cap on the length of any single path before truncation.
    pub max_path_len: usize,
}

impl MatchResult {
    /// Create an empty result with the given caps.
    pub fn new(max_paths: usize, max_path_len: usize) -> Self {
        Self { paths: Vec::new(), max_paths, max_path_len }
    }

    /// Number of emitted paths.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// True if no paths were emitted.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}
