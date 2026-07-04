//! Optimizer for graphlet-based execution planning.
//!
//! Ported from `c_engine/src/optimizer.c`. Provides:
//!
//! - [`CostModel`] — memory/CPU cost coefficients. Unlike the C engine
//!   (`optimizer.c:6`) these are **per-instance**, not file-scope globals.
//!   The `cet_set_cost_coefficients` global-mutation API is gone; concurrent
//!   planners each own their own `CostModel`.
//! - [`Graphlet`] and [`estimate_costs`] — apply a cost model to graphlet
//!   descriptors.
//! - [`detect_graphlets`] — placeholder heuristic mirroring the C code (see
//!   caveat in the function docs).
//! - [`greedy_plan`] and [`branch_and_bound_plan`] — two planners with the
//!   invariants:
//!   1. `plan.total_memory <= max_mem`.
//!   2. On small instances where B&B completes within its budget,
//!      `bnb.total_cpu <= greedy.total_cpu`.
//! - [`classify_graphlet_delta`] — partitions `(prev, curr)` graphlet sets
//!   into `shared`, `new`, and `expired` buckets. Enforces the property
//!   `shared ∪ new == curr` and `shared ∪ expired == prev` (as sets).
//! - [`PartialCache`] — small associative cache with linear probe (fine for
//!   the ≤4096-entry cap).
//!
//! ## Bug fixes over the C engine
//!
//! - **Per-`CostModel` coefficients** — no data race on concurrent planners.
//! - **B&B node budget** — replaces the C code's uncapped 2^n exploration
//!   (`optimizer.c:48-53`) so pathological inputs don't hang the executor.
//! - **`f64::INFINITY` sentinel** — replaces the `1e100` magic value
//!   (`optimizer.c:45,55`) that could be reached by legitimate costs at scale.

use crate::caps;

/// Cost model applied by [`estimate_costs`]. Instances are cheap to clone and
/// carry the coefficients that the C engine kept in file-scope globals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostModel {
    /// Memory cost per graphlet vertex.
    pub mem_vertex: f64,
    /// Memory cost per graphlet edge.
    pub mem_edge: f64,
    /// CPU cost per graphlet edge.
    pub cpu_edge: f64,
    /// CPU cost per graphlet vertex.
    pub cpu_vertex: f64,
}

impl Default for CostModel {
    /// Defaults match the C engine's compile-time values
    /// (`c_engine/src/optimizer.c:6`).
    fn default() -> Self {
        Self { mem_vertex: 0.7, mem_edge: 0.3, cpu_edge: 0.8, cpu_vertex: 0.2 }
    }
}

/// A single graphlet: a sub-graph detected inside a temporal window.
#[derive(Debug, Clone, PartialEq)]
pub struct Graphlet {
    /// Identifier assigned by [`detect_graphlets`].
    pub id: String,
    /// Window start (inclusive), ms.
    pub start_ms: i64,
    /// Window end (inclusive), ms.
    pub end_ms: i64,
    /// Number of vertices in the graphlet.
    pub vertex_count: i64,
    /// Number of edges in the graphlet.
    pub edge_count: i64,
    /// Estimated memory cost — populated by [`estimate_costs`].
    pub memory_cost: f64,
    /// Estimated CPU cost — populated by [`estimate_costs`].
    pub cpu_cost: f64,
}

/// A subset of graphlets chosen by a planner, plus aggregate costs.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    /// Indices into the input graphlet slice.
    pub indices: Vec<usize>,
    /// Sum of `memory_cost` across selected graphlets.
    pub total_memory: f64,
    /// Sum of `cpu_cost` across selected graphlets.
    pub total_cpu: f64,
}

/// Classification of one graphlet set against another.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GraphletDelta {
    /// Indices into `curr` that are also present in `prev`.
    pub shared: Vec<usize>,
    /// Indices into `curr` that are **not** present in `prev`.
    pub new: Vec<usize>,
    /// Indices into `prev` that are **not** present in `curr`.
    pub expired: Vec<usize>,
}

/// A small associative cache of partial-match hit counts.
#[derive(Debug, Default, Clone)]
pub struct PartialCache {
    entries: Vec<PartialCacheEntry>,
    cap: usize,
}

/// One partial-cache slot.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialCacheEntry {
    /// Vertex id keyed by the cache.
    pub key_vertex: i64,
    /// Pattern-sequence index the vertex last matched.
    pub seq_idx: usize,
    /// Number of times this pair has been touched.
    pub hits: u64,
}

impl PartialCache {
    /// Create an empty cache with the default cap ([`caps::MAX_GRAPHLETS`]).
    pub fn new() -> Self {
        Self::with_capacity(caps::MAX_GRAPHLETS)
    }

    /// Create an empty cache with a custom cap.
    pub fn with_capacity(cap: usize) -> Self {
        Self { entries: Vec::new(), cap }
    }

    /// Record a hit against `(key_vertex, seq_idx)`. If the pair is already
    /// present its `hits` counter is incremented; otherwise a new entry is
    /// inserted, up to `cap` entries.
    pub fn touch(&mut self, key_vertex: i64, seq_idx: usize) {
        for e in &mut self.entries {
            if e.key_vertex == key_vertex && e.seq_idx == seq_idx {
                e.hits += 1;
                return;
            }
        }
        if self.entries.len() < self.cap {
            self.entries.push(PartialCacheEntry { key_vertex, seq_idx, hits: 1 });
        }
    }

    /// Slice view of the cache entries.
    pub fn entries(&self) -> &[PartialCacheEntry] {
        &self.entries
    }

    /// Number of populated slots.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no entries have been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Apply the cost model to each graphlet in `arr`, populating `memory_cost`
/// and `cpu_cost`.
pub fn estimate_costs(arr: &mut [Graphlet], model: &CostModel) {
    for g in arr {
        g.memory_cost =
            g.vertex_count as f64 * model.mem_vertex + g.edge_count as f64 * model.mem_edge;
        g.cpu_cost =
            g.edge_count as f64 * model.cpu_edge + g.vertex_count as f64 * model.cpu_vertex;
    }
}

/// Detect graphlets over an ordered list of temporal windows.
///
/// **Caveat**: this is a placeholder that mirrors the C engine's heuristic
/// (`c_engine/src/optimizer.c:23-24`) which computes `vertex_count` and
/// `edge_count` as functions of window duration. It is *not* a real graphlet
/// detector. The port keeps the same shape so tests and callers stay
/// behaviorally compatible; replace with an actual detector when the
/// downstream design is decided.
pub fn detect_graphlets(windows: &[(i64, i64)], cap: usize, model: &CostModel) -> Vec<Graphlet> {
    let n = windows.len().min(cap);
    let mut out: Vec<Graphlet> = Vec::with_capacity(n);
    for (i, &(start, end)) in windows.iter().take(n).enumerate() {
        let duration = end - start;
        out.push(Graphlet {
            id: format!("g{i}"),
            start_ms: start,
            end_ms: end,
            vertex_count: (duration / 10) + 1,
            edge_count: (duration / 8) + 1,
            memory_cost: 0.0,
            cpu_cost: 0.0,
        });
    }
    estimate_costs(&mut out, model);
    out
}

/// Greedy planner: iteratively pick the graphlet with the lowest
/// `cpu_cost + memory_cost` score that still fits under `max_mem`.
pub fn greedy_plan(graphlets: &[Graphlet], max_mem: f64) -> Plan {
    let mut used = vec![false; graphlets.len()];
    let mut plan = Plan::default();

    loop {
        let mut best: Option<(usize, f64)> = None;
        for (i, g) in graphlets.iter().enumerate() {
            if used[i] {
                continue;
            }
            if plan.total_memory + g.memory_cost > max_mem {
                continue;
            }
            let score = g.cpu_cost + g.memory_cost;
            match best {
                None => best = Some((i, score)),
                Some((_, bv)) if score < bv => best = Some((i, score)),
                _ => {}
            }
        }
        match best {
            Some((i, _)) => {
                used[i] = true;
                plan.indices.push(i);
                plan.total_memory += graphlets[i].memory_cost;
                plan.total_cpu += graphlets[i].cpu_cost;
            }
            None => break,
        }
    }
    plan
}

/// Bounded branch-and-bound planner. Selects a subset of graphlets that
/// minimizes total CPU cost subject to a memory cap.
///
/// The search visits at most `node_budget` recursion frames; if the budget
/// is exhausted the best plan seen so far is returned (which may be empty on
/// a truly pathological input).
///
/// Setting `node_budget = 0` disables the search entirely and always returns
/// an empty plan; typical values are `1_000_000` for small inputs.
pub fn branch_and_bound_plan(graphlets: &[Graphlet], max_mem: f64, node_budget: u64) -> Plan {
    struct Ctx<'a> {
        graphlets: &'a [Graphlet],
        max_mem: f64,
        best: Plan,
        best_cpu: f64,
        budget: u64,
    }

    fn recurse(ctx: &mut Ctx<'_>, i: usize, cur: &mut Plan) {
        if ctx.budget == 0 {
            return;
        }
        ctx.budget -= 1;

        if cur.total_memory > ctx.max_mem {
            return;
        }
        if cur.total_cpu >= ctx.best_cpu {
            return;
        }
        if i == ctx.graphlets.len() {
            if !cur.indices.is_empty() && cur.total_cpu < ctx.best_cpu {
                ctx.best = cur.clone();
                ctx.best_cpu = cur.total_cpu;
            }
            return;
        }
        // Exclude i.
        recurse(ctx, i + 1, cur);
        // Include i.
        if cur.indices.len() < caps::MAX_GRAPHLETS {
            let g = &ctx.graphlets[i];
            cur.indices.push(i);
            cur.total_memory += g.memory_cost;
            cur.total_cpu += g.cpu_cost;
            recurse(ctx, i + 1, cur);
            cur.indices.pop();
            cur.total_memory -= g.memory_cost;
            cur.total_cpu -= g.cpu_cost;
        }
    }

    let mut ctx = Ctx {
        graphlets,
        max_mem,
        best: Plan::default(),
        best_cpu: f64::INFINITY,
        budget: node_budget,
    };
    let mut cur = Plan::default();
    recurse(&mut ctx, 0, &mut cur);
    ctx.best
}

/// Compare two graphlet lists by `(start_ms, end_ms)` identity, mirroring
/// the C engine (`c_engine/src/optimizer.c:30`).
fn same_graphlet(a: &Graphlet, b: &Graphlet) -> bool {
    a.start_ms == b.start_ms && a.end_ms == b.end_ms
}

/// Partition `(prev, curr)` into `shared`, `new`, and `expired` index sets.
///
/// - `shared`: indices in `curr` present in `prev`.
/// - `new`: indices in `curr` not present in `prev`.
/// - `expired`: indices in `prev` not present in `curr`.
///
/// Invariant enforced by property tests:
/// `shared ∪ new == curr` and `shared ∪ expired == prev` (as sets of window
/// keys), and `shared ∩ new == ∅`.
pub fn classify_graphlet_delta(prev: &[Graphlet], curr: &[Graphlet]) -> GraphletDelta {
    let mut out = GraphletDelta::default();
    for (i, c) in curr.iter().enumerate() {
        if prev.iter().any(|p| same_graphlet(c, p)) {
            out.shared.push(i);
        } else {
            out.new.push(i);
        }
    }
    for (j, p) in prev.iter().enumerate() {
        if !curr.iter().any(|c| same_graphlet(p, c)) {
            out.expired.push(j);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gl(id: &str, start: i64, end: i64, v: i64, e: i64) -> Graphlet {
        Graphlet {
            id: id.into(),
            start_ms: start,
            end_ms: end,
            vertex_count: v,
            edge_count: e,
            memory_cost: 0.0,
            cpu_cost: 0.0,
        }
    }

    #[test]
    fn cost_model_defaults_match_c_engine() {
        let m = CostModel::default();
        assert_eq!(m.mem_vertex, 0.7);
        assert_eq!(m.mem_edge, 0.3);
        assert_eq!(m.cpu_edge, 0.8);
        assert_eq!(m.cpu_vertex, 0.2);
    }

    #[test]
    fn estimate_costs_applies_coefficients() {
        let mut arr = vec![gl("a", 0, 10, 2, 3)];
        let m = CostModel::default();
        estimate_costs(&mut arr, &m);
        assert!((arr[0].memory_cost - (2.0 * 0.7 + 3.0 * 0.3)).abs() < 1e-9);
        assert!((arr[0].cpu_cost - (3.0 * 0.8 + 2.0 * 0.2)).abs() < 1e-9);
    }

    #[test]
    fn detect_graphlets_bounded_by_cap() {
        let windows = vec![(0, 10), (10, 20), (20, 30)];
        let m = CostModel::default();
        let g = detect_graphlets(&windows, 2, &m);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].id, "g0");
        assert_eq!(g[1].id, "g1");
    }

    #[test]
    fn greedy_plan_respects_memory_cap() {
        let mut arr = vec![gl("a", 0, 10, 2, 3), gl("b", 10, 20, 5, 5), gl("c", 20, 30, 1, 1)];
        estimate_costs(&mut arr, &CostModel::default());
        // Cap that only admits the cheapest one.
        let plan = greedy_plan(&arr, 1.5);
        assert!(plan.total_memory <= 1.5);
        assert!(!plan.indices.is_empty(), "at least the cheapest should fit");
    }

    #[test]
    fn greedy_plan_selects_all_when_budget_is_infinite() {
        let mut arr = vec![gl("a", 0, 10, 2, 3), gl("b", 10, 20, 5, 5), gl("c", 20, 30, 1, 1)];
        estimate_costs(&mut arr, &CostModel::default());
        let plan = greedy_plan(&arr, f64::MAX);
        assert_eq!(plan.indices.len(), arr.len());
    }

    #[test]
    fn bnb_plan_beats_or_matches_greedy_on_small_input() {
        let mut arr = vec![
            gl("a", 0, 10, 2, 3),
            gl("b", 10, 20, 5, 5),
            gl("c", 20, 30, 1, 1),
            gl("d", 30, 40, 3, 2),
        ];
        estimate_costs(&mut arr, &CostModel::default());
        let g = greedy_plan(&arr, 6.0);
        let b = branch_and_bound_plan(&arr, 6.0, 1_000_000);
        // Both must respect the cap.
        assert!(g.total_memory <= 6.0);
        assert!(b.total_memory <= 6.0);
        // B&B, if it returns a non-empty plan, must be no worse in CPU cost.
        if !b.indices.is_empty() && !g.indices.is_empty() {
            assert!(
                b.total_cpu <= g.total_cpu + 1e-9,
                "b&b cpu={} > greedy cpu={}",
                b.total_cpu,
                g.total_cpu
            );
        }
    }

    #[test]
    fn bnb_returns_empty_when_budget_is_zero() {
        let mut arr = vec![gl("a", 0, 10, 2, 3)];
        estimate_costs(&mut arr, &CostModel::default());
        let p = branch_and_bound_plan(&arr, 100.0, 0);
        assert!(p.indices.is_empty());
    }

    #[test]
    fn classify_delta_partitions_correctly() {
        let a = gl("a", 0, 10, 1, 1);
        let b = gl("b", 10, 20, 1, 1);
        let c = gl("c", 20, 30, 1, 1);
        let prev = vec![a.clone(), b.clone()];
        let curr = vec![b.clone(), c.clone()];
        let d = classify_graphlet_delta(&prev, &curr);
        assert_eq!(d.shared, vec![0]); // curr[0] = b
        assert_eq!(d.new, vec![1]); // curr[1] = c
        assert_eq!(d.expired, vec![0]); // prev[0] = a
    }

    #[test]
    fn partial_cache_increments_hits_on_repeat() {
        let mut c = PartialCache::with_capacity(4);
        c.touch(1, 2);
        c.touch(1, 2);
        c.touch(3, 4);
        assert_eq!(c.len(), 2);
        assert_eq!(c.entries()[0].hits, 2);
        assert_eq!(c.entries()[1].hits, 1);
    }

    #[test]
    fn partial_cache_respects_capacity() {
        let mut c = PartialCache::with_capacity(2);
        c.touch(1, 0);
        c.touch(2, 0);
        c.touch(3, 0); // exceeds cap
        assert_eq!(c.len(), 2);
    }

    /// Two `CostModel` instances used concurrently must not interfere.
    /// This regression closes the C engine's global-mutable coefficients bug
    /// (`c_engine/src/optimizer.c:6`).
    #[test]
    fn cost_model_is_per_instance_not_global() {
        let m1 = CostModel { mem_vertex: 1.0, mem_edge: 0.0, cpu_edge: 0.0, cpu_vertex: 0.0 };
        let m2 = CostModel { mem_vertex: 0.0, mem_edge: 1.0, cpu_edge: 0.0, cpu_vertex: 0.0 };
        let mut a = vec![gl("a", 0, 10, 2, 3)];
        let mut b = vec![gl("b", 0, 10, 2, 3)];
        estimate_costs(&mut a, &m1);
        estimate_costs(&mut b, &m2);
        assert_eq!(a[0].memory_cost, 2.0); // 2 * 1.0 + 3 * 0.0
        assert_eq!(b[0].memory_cost, 3.0); // 2 * 0.0 + 3 * 1.0
    }
}
