//! Query model: event-type sequences with optional Kleene-plus and predicates.

use smallvec::SmallVec;

use crate::caps;

/// User-provided predicate that decides whether to advance the pattern from
/// `prev_id` to `curr_id`.
pub type Predicate = std::sync::Arc<dyn Fn(i64, i64) -> bool + Send + Sync>;

/// A single event-type step in a pattern.
#[derive(Clone)]
pub struct EventType {
    /// Event type tag to match against [`crate::graph::Vertex::event_type`].
    pub name: String,
    /// If true, matches one-or-more consecutive events of this type.
    pub kleene_plus: bool,
    /// Optional user-supplied predicate on `(prev_id, curr_id)`.
    pub predicate: Option<Predicate>,
}

impl std::fmt::Debug for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventType")
            .field("name", &self.name)
            .field("kleene_plus", &self.kleene_plus)
            .field("predicate", &self.predicate.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl PartialEq for EventType {
    fn eq(&self, other: &Self) -> bool {
        // Predicates are not comparable; we only compare structural fields.
        self.name == other.name && self.kleene_plus == other.kleene_plus
    }
}

/// A CET query: a name, a sequence of event types, and temporal constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Human-readable query name.
    pub name: String,
    /// Ordered sequence of event types to match.
    pub seq: SmallVec<[EventType; caps::MAX_SEQ]>,
    /// Maximum time span (ms) allowed between the first and last event of a
    /// match. Negative values mean "no bound".
    pub within_ms: i64,
    /// Sliding window step (ms) used by window materialization.
    pub slide_ms: i64,
    /// If true, non-matching neighbors may be skipped when advancing the pattern
    /// (relaxed matching); if false, only strictly matching neighbors advance.
    pub skip_till_any_match: bool,
}

impl Query {
    /// Construct a new query with default flags (`skip_till_any_match = true`).
    pub fn new(name: impl Into<String>, seq: Vec<EventType>) -> Self {
        Self {
            name: name.into(),
            seq: SmallVec::from_vec(seq),
            within_ms: -1,
            slide_ms: 0,
            skip_till_any_match: true,
        }
    }

    /// Number of steps in the pattern.
    pub fn len(&self) -> usize {
        self.seq.len()
    }

    /// True if the pattern is empty.
    pub fn is_empty(&self) -> bool {
        self.seq.is_empty()
    }
}
