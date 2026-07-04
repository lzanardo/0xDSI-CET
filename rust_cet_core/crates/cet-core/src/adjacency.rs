//! Internal adjacency index used by the executors.
//!
//! Replaces the C engine's linear `vpos`/`find_v` scans (`algorithms.c:60,67`)
//! with an `O(1)` structure built once per query:
//!
//! - `head[i]` — first outgoing edge index for vertex at position `i`, or
//!   `NIL` if the vertex has no outgoing edges.
//! - `edges[k]` — flat vector of adjacency records; each record carries the
//!   destination vertex position (precomputed) plus the optional edge window.
//! - `next[k]` — index of the next edge sharing the same source (linked list).
//!
//! Storage is `Vec`-based, so capacity is bounded by the graph, not by a
//! compile-time constant.

use crate::graph::{Edge, Graph};

/// Sentinel for "no more edges".
pub(crate) const NIL: usize = usize::MAX;

/// Single adjacency record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AdjEdge {
    /// Destination vertex position (already resolved from `VertexId`).
    pub dst_pos: usize,
    /// Inclusive edge-window start; ignored if `end <= start`.
    pub window_start_ms: i64,
    /// Inclusive edge-window end.
    pub window_end_ms: i64,
    /// Index of the next edge sharing the same `src_pos`, or [`NIL`].
    pub next: usize,
}

/// Compact adjacency index built once per query.
#[derive(Debug, Default)]
pub(crate) struct AdjIndex {
    /// `head[src_pos]` -> index into `edges`, or [`NIL`].
    pub head: Vec<usize>,
    /// Flat adjacency list.
    pub edges: Vec<AdjEdge>,
}

impl AdjIndex {
    /// Build the adjacency index for `g`.
    ///
    /// Edges whose endpoints are not present are skipped (this can only
    /// happen if a `Graph` is constructed via unsafe paths — safe insertion
    /// via [`Graph::add_edge`] already validates endpoints).
    pub(crate) fn build(g: &Graph) -> Self {
        let n = g.vertex_count();
        let mut head = vec![NIL; n];
        let mut edges: Vec<AdjEdge> = Vec::with_capacity(g.edge_count());

        for e in g.edges() {
            let src_pos = match g.position(e.src) {
                Some(p) => p,
                None => continue,
            };
            let dst_pos = match g.position(e.dst) {
                Some(p) => p,
                None => continue,
            };
            let Edge { window_start_ms, window_end_ms, .. } = *e;

            let idx = edges.len();
            edges.push(AdjEdge { dst_pos, window_start_ms, window_end_ms, next: head[src_pos] });
            head[src_pos] = idx;
        }

        Self { head, edges }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Edge, Graph, Vertex};

    fn v(id: i64, t: i64, ty: &str) -> Vertex {
        Vertex { id, partition_key: "p".into(), event_type: ty.into(), event_time_ms: t }
    }

    #[test]
    fn empty_graph_produces_empty_index() {
        let g = Graph::with_capacity(4, 4);
        let a = AdjIndex::build(&g);
        assert!(a.head.is_empty());
        assert!(a.edges.is_empty());
    }

    #[test]
    fn head_points_to_last_inserted_edge() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        g.add_vertex(v(2, 2, "B")).unwrap();
        g.add_vertex(v(3, 3, "C")).unwrap();
        g.add_edge(Edge { src: 1, dst: 2, window_start_ms: 0, window_end_ms: 0 }).unwrap();
        g.add_edge(Edge { src: 1, dst: 3, window_start_ms: 0, window_end_ms: 0 }).unwrap();
        let a = AdjIndex::build(&g);
        assert_eq!(a.head.len(), 3);
        // Walk the outgoing list for src=1 (position 0) and collect dst ids.
        let mut seen: Vec<i64> = Vec::new();
        let mut ei = a.head[0];
        while ei != NIL {
            let dst_pos = a.edges[ei].dst_pos;
            seen.push(g.vertices()[dst_pos].id);
            ei = a.edges[ei].next;
        }
        seen.sort();
        assert_eq!(seen, vec![2, 3]);
    }
}
