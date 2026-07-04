//! Test-only generators and helpers.
//!
//! Enabled by the `testkit` feature (auto-enabled by the crate's dev-dependencies
//! and by downstream integration tests). Provides `proptest::Strategy`
//! implementations for [`Graph`] and [`Query`] used by the behavior corpus in
//! `tests/behavior.rs`.
//!
//! ## Design notes
//!
//! - Generators produce small, well-formed inputs (≤ 20 vertices, ≤ 40 edges,
//!   sequence length ≤ 4) so property tests remain fast and shrinking is
//!   effective.
//! - Vertices are assigned monotonically increasing `event_time_ms` values.
//! - Edges are generated so that `src` precedes `dst` in time, preserving the
//!   temporal-monotonicity invariant the engine relies on.
//! - Event types are drawn from a small alphabet (`A`, `B`, `C`, `D`) so that
//!   generated queries have a non-trivial chance of matching.

use proptest::collection::vec;
use proptest::prelude::*;

use crate::{Edge, EventType, Graph, Query, Vertex};

/// Alphabet used by [`arb_graph`] and [`arb_query`].
pub const EVENT_TYPES: &[&str] = &["A", "B", "C", "D"];

/// Strategy that produces a well-formed [`Graph`].
///
/// Guarantees:
///
/// - Vertex ids are contiguous starting at 1.
/// - `event_time_ms == vertex_index + 1`, so timestamps are strictly increasing.
/// - Every edge points from an earlier vertex to a later one.
/// - No duplicate `(src, dst)` edges.
pub fn arb_graph() -> impl Strategy<Value = Graph> {
    // First choose the number of vertices, then generate:
    //   - a type per vertex
    //   - a set of edge (src, dst) pairs (dst > src)
    (1usize..=20usize)
        .prop_flat_map(|n| {
            let types = vec(prop::sample::select(EVENT_TYPES), n);
            let edges = vec((0usize..n, 0usize..n), 0..=2 * n);
            (Just(n), types, edges)
        })
        .prop_map(|(n, types, edge_specs)| {
            let mut g = Graph::with_capacity(64, 256);
            for (i, ty) in types.iter().enumerate().take(n) {
                g.add_vertex(Vertex {
                    id: i as i64 + 1,
                    partition_key: "p".to_string(),
                    event_type: (**ty).to_string(),
                    event_time_ms: i as i64 + 1,
                })
                .expect("well-formed vertex");
            }
            let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
            for (a, b) in edge_specs {
                if n < 2 {
                    break;
                }
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                if lo == hi {
                    continue;
                }
                let src = lo as i64 + 1;
                let dst = hi as i64 + 1;
                if !seen.insert((src, dst)) {
                    continue;
                }
                g.add_edge(Edge { src, dst, window_start_ms: 0, window_end_ms: 0 })
                    .expect("well-formed edge");
            }
            g
        })
}

/// Strategy that produces a [`Query`] whose event-type alphabet matches
/// [`EVENT_TYPES`] and whose length is between 1 and 4.
pub fn arb_query() -> impl Strategy<Value = Query> {
    (vec(prop::sample::select(EVENT_TYPES), 1..=4), vec(any::<bool>(), 1..=4), -1i64..=100_000i64)
        .prop_map(|(types, kleenes, within_ms)| {
            let n = types.len().min(kleenes.len());
            let seq: Vec<EventType> = types
                .into_iter()
                .zip(kleenes)
                .take(n)
                .map(|(ty, kleene_plus)| EventType {
                    name: (*ty).to_string(),
                    kleene_plus,
                    predicate: None,
                })
                .collect();
            let mut q = Query::new("prop_q", seq);
            q.within_ms = within_ms;
            q
        })
}

/// Canonicalize a set of paths (sort + dedup) so property tests can compare
/// executor outputs order-independently.
pub fn canonicalize(paths: &[Vec<i64>]) -> Vec<Vec<i64>> {
    let mut owned: Vec<Vec<i64>> = paths.to_vec();
    owned.sort();
    owned.dedup();
    owned
}
