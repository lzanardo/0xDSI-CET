# 0xDSI-CET v7 Standing Trend Runtime

v7 adds a Quine-class user-space standing query layer without abandoning the
Databricks/Delta replay model.

## What it adds

- Mutable temporal graph state.
- Incremental standing CET query evaluation by partition.
- Positive match events.
- Cancel/retraction events when a previously active trend disappears.
- Relation-aware graph evidence (`same_user`, `same_host`, `same_session`,
  `parent_process`, etc.).
- Memory, JSONL, and ZeroBus-style output sinks.
- Databricks streaming notebook and replay job.

## Why this matters

Previous versions were strong for lakehouse replay and batch/microbatch trend
execution. v7 adds the missing live graph behavior: as events mutate graph state,
standing queries continuously produce match/cancel outputs.

## Runtime model

```text
ZeroBus / SDP / Delta events
  -> TemporalGraphState mutation
  -> StandingTrendRuntime
  -> positive_match / cancel_match
  -> Delta / ZeroBus / investigation API
```

## Relation to Quine

Quine remains a mature streaming graph interpreter with native standing queries.
0xDSI-CET v7 focuses on security-specific CET semantics, Delta replay,
Databricks deployment, attack-chain DSL, temporal KG, and agentic investigation.

The long-term direction is not to clone Quine; it is to surpass it for security
operations by combining:

```text
standing graph runtime + CET DSL + Delta replay + SDP + ZeroBus + investigation agents
```
