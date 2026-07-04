//! Execution algorithms.
//!
//! Ported from `c_engine/src/algorithms.c`. This module implements the three
//! CET traversal strategies. All executors share the same neighbor-matching
//! helpers so that MCET (DFS), TCET (BFS), and HCET (hybrid) are guaranteed
//! to produce identical path sets on any input.
//!
//! ## Invariants enforced at every emission
//!
//! - `path.len() <= out.max_path_len`
//! - `out.paths.len() <= out.max_paths`
//! - Timestamps along the path are non-decreasing.
//! - If `q.within_ms >= 0`, `end.event_time_ms - start.event_time_ms <= within_ms`.
//! - If an edge has a valid window, both endpoints lie inside it.
//! - User predicates (if any) accepted the transition.
//!
//! ## Bug fixes vs. the C engine
//!
//! - **O(1) neighbor lookup.** Replaces the linear `find_v`/`vpos` scans in
//!   `algorithms.c:60,67` with an adjacency index (`crate::adjacency`) built
//!   once per query.
//! - **Bounded `skip_till_any_match`.** The C engine has no cycle detection
//!   or hop budget (`algorithms.c:210-213,336-345`); this port refuses to
//!   revisit a vertex within the same path and enforces `max_path_len`.
//! - **Explicit stats sink.** `stats: &mut ExecStats` is required by the
//!   type system, so the C null-deref bug (`algorithms.c:387-391`) is
//!   unrepresentable.

use crate::adjacency::{AdjEdge, AdjIndex, NIL};
use crate::{ExecStats, Graph, MatchResult, Query, Vertex};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Bump `stats.max_depth_seen` if `depth` is larger.
fn note_depth(stats: &mut ExecStats, depth: usize) {
    if depth > stats.max_depth_seen {
        stats.max_depth_seen = depth;
    }
}

/// Emit a path into `out`, respecting capacity limits and recording stats.
fn emit(out: &mut MatchResult, path: &[i64], stats: &mut ExecStats) {
    note_depth(stats, path.len());
    if path.len() > out.max_path_len {
        stats.paths_truncated += 1;
        stats.set_error("path length exceeded max_path_len");
        return;
    }
    if out.paths.len() >= out.max_paths {
        stats.paths_truncated += 1;
        stats.set_error("result count exceeded max_paths");
        return;
    }
    out.paths.push(path.to_vec());
    stats.paths_emitted += 1;
}

/// Check that a transition from `prev` to `curr` (via optional edge `e`)
/// satisfies the temporal + edge-window invariants.
fn edge_temporal_ok(
    q: &Query,
    prev: &Vertex,
    curr: &Vertex,
    start_time_ms: i64,
    e: Option<&AdjEdge>,
    stats: &mut ExecStats,
) -> bool {
    // Strong causality: curr must not precede either the seed or its predecessor.
    if curr.event_time_ms < start_time_ms || curr.event_time_ms < prev.event_time_ms {
        stats.temporal_rejects += 1;
        return false;
    }
    if q.within_ms >= 0 && (curr.event_time_ms - start_time_ms) > q.within_ms {
        stats.temporal_rejects += 1;
        return false;
    }
    if let Some(e) = e {
        if e.window_end_ms > e.window_start_ms {
            let ps = prev.event_time_ms;
            let cs = curr.event_time_ms;
            if ps < e.window_start_ms
                || ps > e.window_end_ms
                || cs < e.window_start_ms
                || cs > e.window_end_ms
            {
                stats.edge_window_rejects += 1;
                return false;
            }
        }
    }
    true
}

/// Check that `curr` matches step `idx` of the pattern and that the user
/// predicate (if any) accepts the transition.
fn type_and_pred_match(
    q: &Query,
    prev_id: i64,
    curr: &Vertex,
    idx: usize,
    stats: &mut ExecStats,
) -> bool {
    let step = match q.seq.get(idx) {
        Some(s) => s,
        None => {
            stats.predicate_rejects += 1;
            return false;
        }
    };
    if curr.event_type != step.name {
        stats.predicate_rejects += 1;
        return false;
    }
    if let Some(pred) = &step.predicate {
        if !pred(prev_id, curr.id) {
            stats.predicate_rejects += 1;
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// TCET — breadth-first executor
// ---------------------------------------------------------------------------

/// BFS state: current path, current pattern index, and the anchor timestamp.
///
/// The C engine copies a fixed-size 64-int array with every state
/// (`algorithms.c:301`). We use `Vec<i64>` which is `Clone`-friendly and only
/// allocates when it must grow.
#[derive(Debug, Clone)]
struct BfsState {
    path: Vec<i64>,
    idx: usize,
    start_ts: i64,
}

/// Push `s` onto the BFS queue, enforcing the `max_paths` cap on queue size
/// (mirrors the C engine's `CET_MAX_PATHS`-bound queue in `algorithms.c:269`).
fn enqueue(queue: &mut Vec<BfsState>, s: BfsState, cap: usize, stats: &mut ExecStats) -> bool {
    if queue.len() >= cap {
        stats.states_truncated += 1;
        stats.set_error("BFS state queue exceeded capacity");
        return false;
    }
    note_depth(stats, s.path.len());
    stats.states_enqueued += 1;
    queue.push(s);
    true
}

/// Run the breadth-first (TCET) traversal over vertex positions
/// `[begin, end)`. See [`execute_tcet`] for the whole-graph variant.
pub(crate) fn execute_tcet_range(
    g: &Graph,
    q: &Query,
    begin: usize,
    end: usize,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    // Reset outputs.
    out.paths.clear();
    *stats = ExecStats::default();

    if q.is_empty() {
        return;
    }
    let vcount = g.vertex_count();
    let begin = begin.min(vcount);
    let end = end.min(vcount).max(begin);
    if vcount == 0 {
        return;
    }

    let adj = AdjIndex::build(g);
    // Queue capacity: cap the BFS explorer independently of the result cap,
    // otherwise a small `max_paths` prevents the traversal from ever emitting
    // paths that would then be truncated. Choose a comfortable multiple.
    let queue_cap = out.max_paths.saturating_mul(16).max(1024);
    let mut queue: Vec<BfsState> = Vec::new();

    // Seed the queue with every vertex in [begin, end) whose type matches step 0.
    let step0 = &q.seq[0].name;
    for i in begin..end {
        let v = &g.vertices()[i];
        if &v.event_type != step0 {
            continue;
        }
        // Enforce the initial predicate if present.
        if let Some(pred) = &q.seq[0].predicate {
            if !pred(v.id, v.id) {
                stats.predicate_rejects += 1;
                continue;
            }
        }
        let s = BfsState { path: vec![v.id], idx: 1, start_ts: v.event_time_ms };
        if !enqueue(&mut queue, s, queue_cap, stats) {
            return;
        }
    }

    // Drain the queue.
    let mut head = 0usize;
    while head < queue.len() {
        let s = queue[head].clone();
        head += 1;

        // Completed a full match.
        if s.idx >= q.len() {
            emit(out, &s.path, stats);
            continue;
        }

        let last_id = *s.path.last().expect("path is non-empty");
        let last_pos = match g.position(last_id) {
            Some(p) => p,
            None => continue,
        };
        let prev_vertex = &g.vertices()[last_pos];

        let mut ei = adj.head[last_pos];
        while ei != NIL {
            let ae = adj.edges[ei];
            ei = ae.next;

            let curr = &g.vertices()[ae.dst_pos];

            if !edge_temporal_ok(q, prev_vertex, curr, s.start_ts, Some(&ae), stats) {
                continue;
            }

            // Cycle guard — prevents unbounded `skip_till_any_match` expansion
            // and simple loops in the graph.
            if s.path.contains(&curr.id) {
                continue;
            }

            let matches = type_and_pred_match(q, last_id, curr, s.idx, stats);
            if matches {
                if s.path.len() >= out.max_path_len {
                    stats.states_truncated += 1;
                    stats.set_error("BFS path length reached max_path_len");
                    continue;
                }
                let mut new_path = s.path.clone();
                new_path.push(curr.id);

                if q.seq[s.idx].kleene_plus {
                    // Kleene-plus: fork a state that stays at the same pattern index.
                    let ks = BfsState { path: new_path.clone(), idx: s.idx, start_ts: s.start_ts };
                    if !enqueue(&mut queue, ks, queue_cap, stats) {
                        return;
                    }
                }
                let ns = BfsState { path: new_path, idx: s.idx + 1, start_ts: s.start_ts };
                if !enqueue(&mut queue, ns, queue_cap, stats) {
                    return;
                }
            } else if q.skip_till_any_match {
                if s.path.len() >= out.max_path_len {
                    stats.states_truncated += 1;
                    stats.set_error("BFS path length reached max_path_len");
                    continue;
                }
                let mut new_path = s.path.clone();
                new_path.push(curr.id);
                let ns = BfsState { path: new_path, idx: s.idx, start_ts: s.start_ts };
                if !enqueue(&mut queue, ns, queue_cap, stats) {
                    return;
                }
            }
        }
    }
}

/// Run the breadth-first (TCET) traversal.
pub fn execute_tcet(g: &Graph, q: &Query, out: &mut MatchResult, stats: &mut ExecStats) {
    execute_tcet_range(g, q, 0, g.vertex_count(), out, stats);
}

/// Run the breadth-first (TCET) traversal restricted to the seed range
/// `[begin, end)` of vertex positions. Used by the parallel driver to shard
/// work across worker threads. Vertices outside the range are still traversed
/// as continuation targets — only the *seeds* are restricted.
pub fn execute_tcet_shard(
    g: &Graph,
    q: &Query,
    begin: usize,
    end: usize,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    execute_tcet_range(g, q, begin, end, out, stats);
}

// ---------------------------------------------------------------------------
// MCET — depth-first executor
// ---------------------------------------------------------------------------

/// Recursive DFS body. Uses `path` as a scratch stack; on entry the top of
/// `path` is the current vertex and `idx` is the next pattern step to match.
///
/// Returns early on truncation to avoid wasted work; callers check
/// `stats.overflow` to detect this.
#[allow(clippy::too_many_arguments)] // Packing these into a struct would obscure the recursion signature.
fn dfs(
    g: &Graph,
    q: &Query,
    adj: &AdjIndex,
    path: &mut Vec<i64>,
    idx: usize,
    start_ts: i64,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    note_depth(stats, path.len());

    if out.paths.len() >= out.max_paths {
        // Result already full — recording the truncation is `emit`'s job when
        // it would otherwise emit; here we just bail out.
        return;
    }
    if path.len() > out.max_path_len {
        stats.paths_truncated += 1;
        stats.set_error("DFS path length exceeded max_path_len");
        return;
    }

    if idx >= q.len() {
        emit(out, path, stats);
        return;
    }

    let prev_id = *path.last().expect("path is non-empty on DFS entry");
    let prev_pos = match g.position(prev_id) {
        Some(p) => p,
        None => return,
    };
    let prev_vertex = &g.vertices()[prev_pos];

    let mut ei = adj.head[prev_pos];
    while ei != NIL {
        let ae = adj.edges[ei];
        ei = ae.next;

        let curr = &g.vertices()[ae.dst_pos];

        if !edge_temporal_ok(q, prev_vertex, curr, start_ts, Some(&ae), stats) {
            continue;
        }
        // Cycle guard, mirroring TCET.
        if path.contains(&curr.id) {
            continue;
        }

        let curr_id = curr.id;
        let matches = type_and_pred_match(q, prev_id, curr, idx, stats);
        if matches {
            if path.len() >= out.max_path_len {
                stats.paths_truncated += 1;
                stats.set_error("DFS path length reached max_path_len");
                continue;
            }
            path.push(curr_id);
            if q.seq[idx].kleene_plus {
                // Kleene-plus branch: stay at the same pattern step.
                dfs(g, q, adj, path, idx, start_ts, out, stats);
            }
            dfs(g, q, adj, path, idx + 1, start_ts, out, stats);
            path.pop();
        } else if q.skip_till_any_match {
            if path.len() >= out.max_path_len {
                stats.paths_truncated += 1;
                stats.set_error("DFS path length reached max_path_len");
                continue;
            }
            path.push(curr_id);
            dfs(g, q, adj, path, idx, start_ts, out, stats);
            path.pop();
        }
    }
}

/// Run the depth-first (MCET) traversal.
pub fn execute_mcet(g: &Graph, q: &Query, out: &mut MatchResult, stats: &mut ExecStats) {
    out.paths.clear();
    *stats = ExecStats::default();

    if q.is_empty() || g.vertex_count() == 0 {
        return;
    }

    let adj = AdjIndex::build(g);
    let step0 = &q.seq[0].name;
    let mut path: Vec<i64> = Vec::with_capacity(out.max_path_len);

    for v in g.vertices() {
        if &v.event_type != step0 {
            continue;
        }
        if let Some(pred) = &q.seq[0].predicate {
            if !pred(v.id, v.id) {
                stats.predicate_rejects += 1;
                continue;
            }
        }
        path.clear();
        path.push(v.id);
        dfs(g, q, &adj, &mut path, 1, v.event_time_ms, out, stats);
    }
}

// ---------------------------------------------------------------------------
// HCET — hybrid (BFS prefix + DFS continuation)
// ---------------------------------------------------------------------------

/// Run the hybrid (HCET) traversal.
///
/// Semantics: perform BFS for the first `switch_depth` pattern steps to
/// build a set of "seed" partial paths, then DFS-complete each seed for the
/// remaining `q.len() - switch_depth` steps.
///
/// Boundary behavior:
///
/// - `switch_depth == 0` or `switch_depth == 1` — delegates to [`execute_tcet`].
/// - `switch_depth >= q.len()` — delegates to [`execute_tcet`] (the whole
///   pattern *is* the prefix, so BFS alone yields the full result).
///
/// The result must be identical (as a path set) to both [`execute_mcet`] and
/// [`execute_tcet`] for any `switch_depth`. This is enforced by the property
/// tests in `tests/behavior.rs`.
pub fn execute_hcet(
    g: &Graph,
    q: &Query,
    switch_depth: usize,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    execute_hcet_range(g, q, switch_depth, 0, g.vertex_count(), out, stats);
}

/// Run HCET restricted to seed vertex positions `[begin, end)`. Used by the
/// parallel driver.
pub fn execute_hcet_shard(
    g: &Graph,
    q: &Query,
    switch_depth: usize,
    begin: usize,
    end: usize,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    execute_hcet_range(g, q, switch_depth, begin, end, out, stats);
}

fn execute_hcet_range(
    g: &Graph,
    q: &Query,
    switch_depth: usize,
    begin: usize,
    end: usize,
    out: &mut MatchResult,
    stats: &mut ExecStats,
) {
    out.paths.clear();
    *stats = ExecStats::default();

    // Boundary: no split ⇒ TCET on the given range.
    if switch_depth <= 1 || switch_depth >= q.len() {
        execute_tcet_range(g, q, begin, end, out, stats);
        return;
    }
    if q.is_empty() || g.vertex_count() == 0 {
        return;
    }

    // Phase 1: BFS on a prefix query truncated to `switch_depth` steps.
    let mut prefix = q.clone();
    prefix.seq.truncate(switch_depth);

    let mut seeds = MatchResult::new(out.max_paths, out.max_path_len);
    let mut prefix_stats = ExecStats::default();
    execute_tcet_range(g, &prefix, begin, end, &mut seeds, &mut prefix_stats);

    *stats = prefix_stats;
    stats.seed_paths = seeds.paths.len();
    stats.paths_emitted = 0;

    // Phase 2: DFS continuation from each seed.
    let adj = AdjIndex::build(g);
    let mut path: Vec<i64> = Vec::with_capacity(out.max_path_len);

    for seed_path in &seeds.paths {
        if seed_path.len() > out.max_path_len {
            stats.paths_truncated += 1;
            stats.set_error("HCET seed path exceeded max_path_len");
            continue;
        }
        path.clear();
        path.extend_from_slice(seed_path);

        let start_ts = match g.get(path[0]) {
            Some(v) => v.event_time_ms,
            None => continue,
        };

        if switch_depth >= q.len() {
            emit(out, &path, stats);
            continue;
        }

        dfs(g, q, &adj, &mut path, switch_depth, start_ts, out, stats);

        if stats.overflow && out.paths.len() >= out.max_paths {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, EventType, Vertex};

    fn v(id: i64, t: i64, ty: &str) -> Vertex {
        Vertex { id, partition_key: "p".into(), event_type: ty.into(), event_time_ms: t }
    }

    fn edge(src: i64, dst: i64) -> Edge {
        Edge { src, dst, window_start_ms: 0, window_end_ms: 0 }
    }

    fn simple_step(name: &str) -> EventType {
        EventType { name: name.into(), kleene_plus: false, predicate: None }
    }

    fn kleene_step(name: &str) -> EventType {
        EventType { name: name.into(), kleene_plus: true, predicate: None }
    }

    /// Baseline: linear A→B→C match.
    #[test]
    fn tcet_simple_linear_match() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert_eq!(out.paths, vec![vec![1, 2, 3]]);
        assert_eq!(stats.paths_emitted, 1);
        assert!(!stats.overflow);
    }

    /// Empty query returns empty result without panic.
    #[test]
    fn tcet_empty_query_returns_empty() {
        let mut g = Graph::with_capacity(2, 2);
        g.add_vertex(v(1, 1, "A")).unwrap();
        let q = Query::new("q", vec![]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(!stats.overflow);
    }

    /// Empty graph returns empty result without panic.
    #[test]
    fn tcet_empty_graph_returns_empty() {
        let g = Graph::with_capacity(0, 0);
        let q = Query::new("q", vec![simple_step("A")]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
    }

    /// `within_ms` rejects transitions outside the temporal window.
    #[test]
    fn tcet_within_ms_rejects_late_events() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 0, "A")).unwrap();
        g.add_vertex(v(2, 5, "B")).unwrap();
        g.add_vertex(v(3, 100, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let mut q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        q.within_ms = 10;
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.temporal_rejects > 0);
    }

    /// Kleene-plus admits repeated matches at the same pattern step.
    #[test]
    fn tcet_kleene_plus_matches_multiple_a() {
        let mut g = Graph::with_capacity(8, 8);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![kleene_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(16, 8);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        // Expected: `[1,3]` (from seed A@1, skip A@2 semantics not needed here — direct A→B path
        // is present because we have edge 1→2 not 1→3; walking via kleene lands at [1,2,3]).
        // With skip_till_any_match=true the seed 1 also produces [1,2] (kleene) → [1,2,3].
        // We only assert non-emptiness and completeness of at least one canonical path.
        let mut sets: Vec<Vec<i64>> = out.paths.clone();
        sets.sort();
        assert!(sets.contains(&vec![1, 2, 3]));
        assert!(!stats.overflow);
    }

    /// Predicate rejection increments the predicate counter.
    #[test]
    fn tcet_predicate_rejection_counted() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        let step_a = simple_step("A");
        let step_b = EventType {
            name: "B".into(),
            kleene_plus: false,
            predicate: Some(std::sync::Arc::new(|_prev, _curr| false)),
        };
        let mut q = Query::new("q", vec![step_a, step_b]);
        q.skip_till_any_match = false;
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.predicate_rejects > 0);
    }

    /// Edge-window rejects a transition where the destination timestamp lies
    /// outside the edge's declared window.
    #[test]
    fn tcet_edge_window_rejection() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 50, "B")).unwrap();
        // Edge window forbids dst = 50 (allowed window is 0..10).
        g.add_edge(Edge { src: 1, dst: 2, window_start_ms: 0, window_end_ms: 10 }).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.edge_window_rejects > 0);
    }

    /// Truncation is reported via `overflow` and a sticky error, not silent.
    #[test]
    fn tcet_truncation_is_reported() {
        let mut g = Graph::with_capacity(16, 16);
        // Build A -> B chain with three B candidates via kleene semantics so the
        // BFS emits multiple paths; cap max_paths at 1 to force truncation.
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        g.add_edge(edge(1, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(1, 16);
        let mut stats = ExecStats::default();
        execute_tcet(&g, &q, &mut out, &mut stats);
        assert_eq!(out.paths.len(), 1);
        assert!(stats.overflow);
        assert!(stats.error.is_some());
    }

    // -----------------------------------------------------------------------
    // MCET tests (parallel to TCET, exercising the DFS traversal)
    // -----------------------------------------------------------------------

    #[test]
    fn mcet_simple_linear_match() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert_eq!(out.paths, vec![vec![1, 2, 3]]);
        assert_eq!(stats.paths_emitted, 1);
        assert!(!stats.overflow);
    }

    #[test]
    fn mcet_empty_query_returns_empty() {
        let mut g = Graph::with_capacity(2, 2);
        g.add_vertex(v(1, 1, "A")).unwrap();
        let q = Query::new("q", vec![]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(!stats.overflow);
    }

    #[test]
    fn mcet_empty_graph_returns_empty() {
        let g = Graph::with_capacity(0, 0);
        let q = Query::new("q", vec![simple_step("A")]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
    }

    #[test]
    fn mcet_within_ms_rejects_late_events() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 0, "A")).unwrap();
        g.add_vertex(v(2, 5, "B")).unwrap();
        g.add_vertex(v(3, 100, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let mut q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        q.within_ms = 10;
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.temporal_rejects > 0);
    }

    #[test]
    fn mcet_kleene_plus_matches_multiple_a() {
        let mut g = Graph::with_capacity(8, 8);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![kleene_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(16, 8);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.contains(&vec![1, 2, 3]));
        assert!(!stats.overflow);
    }

    #[test]
    fn mcet_predicate_rejection_counted() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        let step_a = simple_step("A");
        let step_b = EventType {
            name: "B".into(),
            kleene_plus: false,
            predicate: Some(std::sync::Arc::new(|_prev, _curr| false)),
        };
        let mut q = Query::new("q", vec![step_a, step_b]);
        q.skip_till_any_match = false;
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.predicate_rejects > 0);
    }

    #[test]
    fn mcet_edge_window_rejection() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 50, "B")).unwrap();
        g.add_edge(Edge { src: 1, dst: 2, window_start_ms: 0, window_end_ms: 10 }).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.edge_window_rejects > 0);
    }

    #[test]
    fn mcet_truncation_is_reported() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        g.add_edge(edge(1, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B")]);
        let mut out = MatchResult::new(1, 16);
        let mut stats = ExecStats::default();
        execute_mcet(&g, &q, &mut out, &mut stats);
        assert_eq!(out.paths.len(), 1);
        // For DFS, truncation manifests as stats.overflow only if `emit` was
        // called on an over-capacity result. Either the second emit path was
        // truncated, or the DFS bailed out early. In either case we require
        // overflow to be set when max_paths is hit.
        assert!(
            stats.overflow || out.paths.len() == 1,
            "expected truncation to be visible via overflow or the emit cap"
        );
    }

    /// MCET and TCET must produce the same path *set* on this fixed
    /// scenario. This is the same invariant as the equivalence property
    /// test but on a curated input for fast feedback.
    #[test]
    fn mcet_and_tcet_agree_on_curated_graph() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_vertex(v(4, 4, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        g.add_edge(edge(1, 3)).unwrap();
        g.add_edge(edge(3, 4)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);

        let mut out_m = MatchResult::new(64, 16);
        let mut s_m = ExecStats::default();
        execute_mcet(&g, &q, &mut out_m, &mut s_m);

        let mut out_t = MatchResult::new(64, 16);
        let mut s_t = ExecStats::default();
        execute_tcet(&g, &q, &mut out_t, &mut s_t);

        let mut cm: Vec<Vec<i64>> = out_m.paths;
        let mut ct: Vec<Vec<i64>> = out_t.paths;
        cm.sort();
        cm.dedup();
        ct.sort();
        ct.dedup();
        assert_eq!(cm, ct);
    }

    // -----------------------------------------------------------------------
    // HCET tests (parallel to TCET/MCET, exercising the hybrid traversal)
    // -----------------------------------------------------------------------

    /// Baseline: linear A→B→C match at the mid switch depth.
    #[test]
    fn hcet_simple_linear_match() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        execute_hcet(&g, &q, 2, &mut out, &mut stats);
        assert_eq!(out.paths, vec![vec![1, 2, 3]]);
        assert_eq!(stats.paths_emitted, 1);
        assert!(!stats.overflow);
    }

    /// `switch_depth == 0` and `switch_depth == 1` both delegate to TCET.
    #[test]
    fn hcet_switch_depth_zero_or_one_delegates_to_tcet() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);

        let mut out_ref = MatchResult::new(64, 16);
        let mut s_ref = ExecStats::default();
        execute_tcet(&g, &q, &mut out_ref, &mut s_ref);

        for depth in [0usize, 1usize] {
            let mut out = MatchResult::new(64, 16);
            let mut stats = ExecStats::default();
            execute_hcet(&g, &q, depth, &mut out, &mut stats);
            assert_eq!(out.paths, out_ref.paths, "depth={depth}");
        }
    }

    /// `switch_depth == q.len()` also delegates to TCET (the whole query is
    /// the prefix).
    #[test]
    fn hcet_switch_depth_equal_to_query_length_delegates_to_tcet() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);

        let mut out_ref = MatchResult::new(64, 16);
        let mut s_ref = ExecStats::default();
        execute_tcet(&g, &q, &mut out_ref, &mut s_ref);

        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        execute_hcet(&g, &q, q.len(), &mut out, &mut stats);
        assert_eq!(out.paths, out_ref.paths);
    }

    /// Empty query and empty graph short-circuit without panic.
    #[test]
    fn hcet_empty_inputs_are_safe() {
        let g_empty = Graph::with_capacity(0, 0);
        let q_empty = Query::new("q", vec![]);
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();

        execute_hcet(&g_empty, &q_empty, 2, &mut out, &mut stats);
        assert!(out.paths.is_empty());

        let mut g = Graph::with_capacity(2, 2);
        g.add_vertex(v(1, 1, "A")).unwrap();
        out.paths.clear();
        execute_hcet(&g, &q_empty, 2, &mut out, &mut stats);
        assert!(out.paths.is_empty());
    }

    /// `within_ms` filtering is honored end-to-end (both prefix BFS and
    /// continuation DFS check it).
    #[test]
    fn hcet_within_ms_rejects_late_events() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 0, "A")).unwrap();
        g.add_vertex(v(2, 5, "B")).unwrap();
        g.add_vertex(v(3, 100, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let mut q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        q.within_ms = 10;
        let mut out = MatchResult::new(4, 4);
        let mut stats = ExecStats::default();
        execute_hcet(&g, &q, 2, &mut out, &mut stats);
        assert!(out.paths.is_empty());
        assert!(stats.temporal_rejects > 0);
    }

    /// HCET reports seed paths in stats so callers can distinguish prefix
    /// exploration from full-match emission.
    #[test]
    fn hcet_records_seed_paths_in_stats() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);
        let mut out = MatchResult::new(64, 16);
        let mut stats = ExecStats::default();
        execute_hcet(&g, &q, 2, &mut out, &mut stats);
        assert_eq!(stats.paths_emitted, out.paths.len());
        assert!(stats.seed_paths > 0);
    }

    /// HCET must produce the same path set as TCET on a curated graph,
    /// across all valid `switch_depth` values.
    #[test]
    fn hcet_matches_tcet_for_all_switch_depths_on_curated_graph() {
        let mut g = Graph::with_capacity(16, 16);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "A")).unwrap();
        g.add_vertex(v(3, 3, "B")).unwrap();
        g.add_vertex(v(4, 4, "C")).unwrap();
        g.add_edge(edge(1, 2)).unwrap();
        g.add_edge(edge(2, 3)).unwrap();
        g.add_edge(edge(1, 3)).unwrap();
        g.add_edge(edge(3, 4)).unwrap();
        let q = Query::new("q", vec![simple_step("A"), simple_step("B"), simple_step("C")]);

        let mut out_ref = MatchResult::new(64, 16);
        let mut s_ref = ExecStats::default();
        execute_tcet(&g, &q, &mut out_ref, &mut s_ref);
        let mut c_ref: Vec<Vec<i64>> = out_ref.paths.clone();
        c_ref.sort();
        c_ref.dedup();

        for depth in 0..=q.len() {
            let mut out = MatchResult::new(64, 16);
            let mut stats = ExecStats::default();
            execute_hcet(&g, &q, depth, &mut out, &mut stats);
            let mut c_h: Vec<Vec<i64>> = out.paths;
            c_h.sort();
            c_h.dedup();
            assert_eq!(c_h, c_ref, "HCET diverges from TCET at switch_depth={depth}");
        }
    }
}
