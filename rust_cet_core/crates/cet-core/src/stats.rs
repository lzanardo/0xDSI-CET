//! Execution diagnostics.
//!
//! Mirrors the C engine's `cet_exec_stats_t` (`c_engine/include/cet.h`) but uses
//! `Option<String>` for the error message instead of a fixed 256-byte buffer.

/// Diagnostic counters emitted during query execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecStats {
    /// Number of paths successfully emitted into the result set.
    pub paths_emitted: usize,
    /// Number of paths that were dropped because a capacity was reached.
    pub paths_truncated: usize,
    /// Number of BFS states enqueued.
    pub states_enqueued: usize,
    /// Number of BFS states that could not be enqueued due to capacity.
    pub states_truncated: usize,
    /// Number of seed paths produced by the H-CET prefix phase.
    pub seed_paths: usize,
    /// Maximum depth reached in any traversal.
    pub max_depth_seen: usize,
    /// Number of neighbors rejected by the temporal-monotonicity check.
    pub temporal_rejects: usize,
    /// Number of neighbors rejected by edge-window checks.
    pub edge_window_rejects: usize,
    /// Number of neighbors rejected by user predicates.
    pub predicate_rejects: usize,
    /// True if any truncation or overflow occurred.
    pub overflow: bool,
    /// First error message recorded (mirrors the C engine's "sticky first
    /// error" contract).
    pub error: Option<String>,
}

impl ExecStats {
    /// Record a truncation event with a message. First message wins.
    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.overflow = true;
        if self.error.is_none() {
            self.error = Some(msg.into());
        }
    }

    /// Merge another stats struct into `self`. Used by the parallel executor
    /// to fold per-worker diagnostics.
    pub fn merge(&mut self, other: &ExecStats) {
        self.paths_emitted += other.paths_emitted;
        self.paths_truncated += other.paths_truncated;
        self.states_enqueued += other.states_enqueued;
        self.states_truncated += other.states_truncated;
        self.seed_paths += other.seed_paths;
        self.max_depth_seen = self.max_depth_seen.max(other.max_depth_seen);
        self.temporal_rejects += other.temporal_rejects;
        self.edge_window_rejects += other.edge_window_rejects;
        self.predicate_rejects += other.predicate_rejects;
        self.overflow |= other.overflow;
        if self.error.is_none() {
            self.error.clone_from(&other.error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_error_is_sticky() {
        let mut s = ExecStats::default();
        s.set_error("first");
        s.set_error("second");
        assert_eq!(s.error.as_deref(), Some("first"));
        assert!(s.overflow);
    }

    #[test]
    fn merge_preserves_first_error_and_max_depth() {
        let mut a = ExecStats { max_depth_seen: 3, ..Default::default() };
        a.set_error("A");
        let mut b = ExecStats { max_depth_seen: 7, ..Default::default() };
        b.set_error("B");
        a.merge(&b);
        assert_eq!(a.max_depth_seen, 7);
        assert_eq!(a.error.as_deref(), Some("A"));
        assert!(a.overflow);
    }
}
