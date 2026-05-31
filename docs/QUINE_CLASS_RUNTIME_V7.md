# Quine-class runtime gap closure in v7

v7 closes the largest architectural gap versus Quine: standing query behavior.

## Covered in v7

- Long-lived graph state in user-space.
- Standing query registry over CET DSL queries.
- Incremental re-evaluation on graph mutations.
- Positive/cancel trend events.
- Relation-aware evidence graph.
- ZeroBus sink hooks.
- Databricks replay and streaming jobs.

## Still different from Quine

Quine uses a graph-native interpreter and deeply integrated standing query
propagation. v7 uses deterministic partition-scoped re-evaluation. That is
simpler, replayable, Delta-friendly, and easier to govern in security
operations, but it is not yet a fully actor-based distributed graph VM.

## Next possible optimization

- Native C edge-label traversal.
- Predicate pushdown into the C kernel.
- Partition-sharded long-lived services outside Spark tasks.
- RocksDB/Delta hybrid graph-state backend.
- Web UI for evidence graph exploration.
