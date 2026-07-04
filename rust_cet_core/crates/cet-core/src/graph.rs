//! Event graph data model.
//!
//! The graph is stored as a vector of vertices and a vector of edges, plus a
//! `HashMap<VertexId, usize>` index that eliminates the O(V) `find_v`/`vpos`
//! scans present in the C engine (`c_engine/src/algorithms.c:60,67`).

use ahash::AHashMap;

use crate::error::{CetError, CetResult};

/// Public vertex identifier. Kept as `i64` for FFI compatibility with the C
/// engine's `int` while giving us headroom in Rust-native call sites.
pub type VertexId = i64;

/// A single event vertex in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vertex {
    /// External identifier of the event.
    pub id: VertexId,
    /// Partition key (e.g. user id, session id).
    pub partition_key: String,
    /// Event type tag matched by [`crate::Query`] sequences.
    pub event_type: String,
    /// Event timestamp in milliseconds since epoch.
    pub event_time_ms: i64,
}

/// A directed edge between two events, optionally restricted to a time window.
///
/// When `window_end_ms > window_start_ms` the edge is only considered active for
/// pairs of events whose timestamps fall inside `[window_start_ms, window_end_ms]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Source vertex id.
    pub src: VertexId,
    /// Destination vertex id.
    pub dst: VertexId,
    /// Inclusive window start; ignored if `window_end_ms <= window_start_ms`.
    pub window_start_ms: i64,
    /// Inclusive window end; ignored if `window_end_ms <= window_start_ms`.
    pub window_end_ms: i64,
}

/// An event graph.
#[derive(Debug, Default, Clone)]
pub struct Graph {
    vertices: Vec<Vertex>,
    edges: Vec<Edge>,
    /// Index from vertex id to position in `vertices`. Kept in sync by
    /// [`Graph::add_vertex`]. Solves the linear-scan bug in the C engine.
    index: AHashMap<VertexId, usize>,
    max_vertices: usize,
    max_edges: usize,
}

impl Graph {
    /// Create an empty graph with the given capacity limits.
    ///
    /// The C engine hardcodes these to `CET_MAX_EVENTS` and `CET_MAX_EDGES`.
    /// In Rust we make them explicit so tests can construct small graphs
    /// without allocating megabytes.
    pub fn with_capacity(max_vertices: usize, max_edges: usize) -> Self {
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            index: AHashMap::new(),
            max_vertices,
            max_edges,
        }
    }

    /// Insert a vertex. Returns [`CetError::CapacityExceeded`] when the graph
    /// is full and [`CetError::DuplicateVertex`] if the id was already
    /// inserted (unlike the C engine, which silently accepts duplicates).
    pub fn add_vertex(&mut self, v: Vertex) -> CetResult<()> {
        if self.vertices.len() >= self.max_vertices {
            return Err(CetError::CapacityExceeded { what: "vertices", limit: self.max_vertices });
        }
        if self.index.contains_key(&v.id) {
            return Err(CetError::DuplicateVertex(v.id));
        }
        self.index.insert(v.id, self.vertices.len());
        self.vertices.push(v);
        Ok(())
    }

    /// Insert an edge. Both endpoints must already exist (the C engine
    /// silently drops edges with unknown src at `build_adj` time; we surface
    /// the error at insert time).
    pub fn add_edge(&mut self, e: Edge) -> CetResult<()> {
        if self.edges.len() >= self.max_edges {
            return Err(CetError::CapacityExceeded { what: "edges", limit: self.max_edges });
        }
        if !self.index.contains_key(&e.src) {
            return Err(CetError::UnknownVertex(e.src));
        }
        if !self.index.contains_key(&e.dst) {
            return Err(CetError::UnknownVertex(e.dst));
        }
        self.edges.push(e);
        Ok(())
    }

    /// Total number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Slice view of the vertex array.
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Slice view of the edge array.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// O(1) lookup of a vertex by id.
    pub fn get(&self, id: VertexId) -> Option<&Vertex> {
        self.index.get(&id).map(|&i| &self.vertices[i])
    }

    /// O(1) lookup of a vertex position by id.
    pub fn position(&self, id: VertexId) -> Option<usize> {
        self.index.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: i64, t: i64, ty: &str) -> Vertex {
        Vertex { id, partition_key: "p".into(), event_type: ty.into(), event_time_ms: t }
    }

    #[test]
    fn add_vertex_ok() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        assert_eq!(g.vertex_count(), 1);
        assert_eq!(g.get(1).unwrap().event_type, "A");
        assert_eq!(g.position(1), Some(0));
    }

    #[test]
    fn duplicate_vertex_rejected() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        assert_eq!(g.add_vertex(v(1, 2, "B")), Err(CetError::DuplicateVertex(1)));
    }

    #[test]
    fn vertex_capacity_enforced() {
        let mut g = Graph::with_capacity(1, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        assert!(matches!(
            g.add_vertex(v(2, 2, "B")),
            Err(CetError::CapacityExceeded { what: "vertices", .. })
        ));
    }

    #[test]
    fn edge_requires_known_endpoints() {
        let mut g = Graph::with_capacity(4, 4);
        g.add_vertex(v(1, 1, "A")).unwrap();
        let e = Edge { src: 1, dst: 99, window_start_ms: 0, window_end_ms: 0 };
        assert_eq!(g.add_edge(e), Err(CetError::UnknownVertex(99)));
    }
}
