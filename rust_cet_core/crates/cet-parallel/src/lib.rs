//! # cet-parallel
//!
//! Parallel driver for the H-CET executor. Replaces the C engine's manual
//! `pthread_create` bookkeeping (`c_engine/src/parallel_hcet.c`) with a Rayon
//! thread pool. The public API mirrors `cet_execute_hcet_parallel_ex` and
//! exposes a [`RuntimeConfig`] / [`RuntimeStats`] pair.
//!
//! ## Determinism
//!
//! Workers process disjoint seed-vertex ranges and produce independent path
//! sets. The merge concatenates shard results in **shard index order**, so
//! the final path order is stable across thread counts and OS scheduling.
//! This property is enforced by `property_parallel_matches_serial` in
//! `crates/cet-core/tests/behavior.rs`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use cet_core::{caps, exec, ExecStats, Graph, MatchResult, Query};
use rayon::prelude::*;

/// Runtime configuration for the parallel executor.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Number of worker threads. `0` and `1` disable parallelism.
    pub native_threads: usize,
    /// If true, merge worker outputs in a deterministic order. Currently
    /// always true (shards are merged in index order); the field is kept
    /// for parity with the C engine and future opt-out.
    pub deterministic_merge: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { native_threads: 1, deterministic_merge: true }
    }
}

/// Diagnostic counters emitted by the parallel driver.
#[derive(Debug, Default, Clone)]
pub struct RuntimeStats {
    /// Threads requested by the caller (possibly clamped downstream).
    pub native_threads_requested: usize,
    /// Threads actually used.
    pub native_threads_used: usize,
    /// True if a parallel path was taken (vs. the single-thread fallback).
    pub parallel_enabled: bool,
    /// First error message recorded, if any.
    pub error: Option<String>,
}

impl RuntimeStats {
    /// Set the sticky first-error message.
    pub fn set_error(&mut self, msg: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(msg.into());
        }
    }
}

/// Execute an H-CET query in parallel.
///
/// The graph is sharded by seed-vertex position across `cfg.native_threads`
/// workers; each worker runs the serial [`exec::execute_hcet_shard`] on its
/// range. Results are merged in shard index order for determinism.
///
/// Falls back to serial execution when:
///
/// - `cfg.native_threads <= 1`
/// - the graph has fewer vertices than requested threads
/// - the query is empty
pub fn execute_hcet_parallel(
    g: &Graph,
    q: &Query,
    switch_depth: usize,
    cfg: &RuntimeConfig,
    out: &mut MatchResult,
    stats: &mut ExecStats,
    rt: &mut RuntimeStats,
) {
    out.paths.clear();
    *stats = ExecStats::default();
    *rt = RuntimeStats::default();
    rt.native_threads_requested = cfg.native_threads;

    let vcount = g.vertex_count();
    let requested = cfg.native_threads.clamp(1, caps::MAX_NATIVE_THREADS);
    let workers = requested.min(vcount.max(1));

    if workers <= 1 || vcount == 0 || q.is_empty() {
        rt.native_threads_used = 1;
        rt.parallel_enabled = false;
        exec::execute_hcet(g, q, switch_depth, out, stats);
        return;
    }

    rt.native_threads_used = workers;
    rt.parallel_enabled = true;

    // Build shard ranges: [begin_i, end_i) for i in 0..workers, covering [0, vcount).
    let base = vcount / workers;
    let rem = vcount % workers;
    let ranges: Vec<(usize, usize)> = (0..workers)
        .scan(0usize, |cursor, i| {
            let span = base + if i < rem { 1 } else { 0 };
            let begin = *cursor;
            let end = begin + span;
            *cursor = end;
            Some((begin, end))
        })
        .collect();

    // Build a private rayon pool so `native_threads` is strictly honored and
    // we do not oversubscribe the global pool (important on Spark/Databricks
    // executors that already have their own thread budget).
    let pool = match rayon::ThreadPoolBuilder::new().num_threads(workers).build() {
        Ok(p) => p,
        Err(_) => {
            rt.set_error("rayon pool creation failed; falling back to serial");
            rt.native_threads_used = 1;
            rt.parallel_enabled = false;
            exec::execute_hcet(g, q, switch_depth, out, stats);
            return;
        }
    };

    // Per-shard result + stats. Owned by main thread; workers borrow &mut.
    let mut shard_results: Vec<(MatchResult, ExecStats)> = (0..workers)
        .map(|_| (MatchResult::new(out.max_paths, out.max_path_len), ExecStats::default()))
        .collect();

    pool.install(|| {
        shard_results.par_iter_mut().zip(ranges.par_iter()).for_each(
            |((shard_out, shard_stats), &(begin, end))| {
                exec::execute_hcet_shard(g, q, switch_depth, begin, end, shard_out, shard_stats);
            },
        );
    });

    // Deterministic merge (shard index order).
    for (shard_out, shard_stats) in shard_results {
        stats.merge(&shard_stats);
        for p in shard_out.paths {
            if out.paths.len() >= out.max_paths {
                stats.paths_truncated += 1;
                stats.set_error("parallel merge exceeded max_paths");
                continue;
            }
            if p.len() > out.max_path_len {
                stats.paths_truncated += 1;
                stats.set_error("parallel merge encountered path longer than max_path_len");
                continue;
            }
            out.paths.push(p);
        }
    }
    // After merge, paths_emitted reflects the actual final result length.
    stats.paths_emitted = out.paths.len();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cet_core::{Edge, EventType, Vertex};

    fn v(id: i64, t: i64, ty: &str) -> Vertex {
        Vertex { id, partition_key: "p".into(), event_type: ty.into(), event_time_ms: t }
    }

    fn e(src: i64, dst: i64) -> Edge {
        Edge { src, dst, window_start_ms: 0, window_end_ms: 0 }
    }

    fn step(name: &str) -> EventType {
        EventType { name: name.into(), kleene_plus: false, predicate: None }
    }

    fn chain_graph() -> (Graph, Query) {
        let mut g = Graph::with_capacity(32, 32);
        // A(1) -> A(2) -> B(3) -> C(4), plus a direct A(1) -> B(3) shortcut.
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_vertex(v(4, 4, "C")).unwrap();
        g.add_edge(e(1, 2)).unwrap();
        g.add_edge(e(2, 3)).unwrap();
        g.add_edge(e(1, 3)).unwrap();
        g.add_edge(e(3, 4)).unwrap();
        let q = Query::new("q", vec![step("A"), step("B"), step("C")]);
        (g, q)
    }

    #[test]
    fn default_config_is_single_thread() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.native_threads, 1);
        assert!(cfg.deterministic_merge);
    }

    #[test]
    fn runtime_stats_error_is_sticky() {
        let mut rt = RuntimeStats::default();
        rt.set_error("first");
        rt.set_error("second");
        assert_eq!(rt.error.as_deref(), Some("first"));
    }

    /// `native_threads <= 1` takes the serial path.
    #[test]
    fn threads_one_falls_back_to_serial() {
        let (g, q) = chain_graph();
        let cfg = RuntimeConfig { native_threads: 1, deterministic_merge: true };
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        let mut rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, 2, &cfg, &mut out, &mut stats, &mut rt);
        assert_eq!(rt.native_threads_used, 1);
        assert!(!rt.parallel_enabled);
        assert!(!out.paths.is_empty());
    }

    /// Parallel execution with 4 workers produces the same path set as
    /// serial HCET.
    #[test]
    #[cfg_attr(miri, ignore = "miri does not support rayon's OS threads")]
    fn threads_four_matches_serial() {
        let (g, q) = chain_graph();

        let mut serial_out = MatchResult::new(64, 16);
        let mut serial_stats = ExecStats::default();
        exec::execute_hcet(&g, &q, 2, &mut serial_out, &mut serial_stats);

        let cfg = RuntimeConfig { native_threads: 4, deterministic_merge: true };
        let mut par_out = MatchResult::new(64, 16);
        let mut par_stats = ExecStats::default();
        let mut rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, 2, &cfg, &mut par_out, &mut par_stats, &mut rt);

        assert_eq!(rt.native_threads_requested, 4);
        assert!(rt.parallel_enabled);
        assert!(rt.native_threads_used > 1);

        let mut a: Vec<Vec<i64>> = serial_out.paths;
        let mut b: Vec<Vec<i64>> = par_out.paths;
        a.sort();
        a.dedup();
        b.sort();
        b.dedup();
        assert_eq!(a, b);
    }

    /// Repeated parallel runs with the same thread count produce
    /// bit-identical output (deterministic merge).
    #[test]
    #[cfg_attr(miri, ignore = "miri does not support rayon's OS threads")]
    fn parallel_runs_are_deterministic() {
        let (g, q) = chain_graph();
        let cfg = RuntimeConfig { native_threads: 4, deterministic_merge: true };
        let mut first: Option<Vec<Vec<i64>>> = None;
        for _ in 0..5 {
            let mut out = MatchResult::new(64, 16);
            let mut stats = ExecStats::default();
            let mut rt = RuntimeStats::default();
            execute_hcet_parallel(&g, &q, 2, &cfg, &mut out, &mut stats, &mut rt);
            match &first {
                None => first = Some(out.paths),
                Some(prev) => assert_eq!(prev, &out.paths),
            }
        }
    }

    /// Empty graph and empty query short-circuit safely.
    #[test]
    fn empty_inputs_are_safe() {
        let g = Graph::with_capacity(0, 0);
        let q = Query::new("q", vec![step("A")]);
        let cfg = RuntimeConfig { native_threads: 4, deterministic_merge: true };
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        let mut rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, 2, &cfg, &mut out, &mut stats, &mut rt);
        assert!(out.paths.is_empty());
        assert_eq!(rt.native_threads_used, 1);
    }

    /// `native_threads > MAX_NATIVE_THREADS` is clamped.
    #[test]
    #[cfg_attr(miri, ignore = "miri does not support rayon's OS threads")]
    fn thread_count_is_clamped() {
        let (g, q) = chain_graph();
        let cfg = RuntimeConfig {
            native_threads: caps::MAX_NATIVE_THREADS + 100,
            deterministic_merge: true,
        };
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        let mut rt = RuntimeStats::default();
        execute_hcet_parallel(&g, &q, 2, &cfg, &mut out, &mut stats, &mut rt);
        assert!(rt.native_threads_used <= caps::MAX_NATIVE_THREADS);
    }
}
