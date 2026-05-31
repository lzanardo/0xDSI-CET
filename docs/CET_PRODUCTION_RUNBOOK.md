# 0xDSI-CET Production Runbook

## Normal streaming

Use `notebooks/0xDSI_CET_Databricks_prod.py`.

Required source table:

```text
main.cet.cet_events
```

Required columns:

```text
event_id BIGINT
partition_key STRING
event_type STRING
event_time_ms BIGINT
event_time TIMESTAMP
```

## Query management

Queries live in:

```text
main.cet.cet_queries
```

Default seeded query:

```text
security_escalation v1 AuthFail+,PrivEsc,DataAccess WITHIN 30m SLIDE 5m
```

Breaking query changes require a new `query_version` and a replay.

## State model

The runtime persists event-buffer state in:

```text
main.cet.cet_event_buffer
```

This is deliberately simple and robust: the system keeps enough recent events
per query/partition to detect patterns that cross microbatch boundaries.

## Replay

Replay is required when:

- a late event lands after the normal state window,
- corrected events are backfilled,
- a query version is reprocessed,
- raw event semantics change.

Replay sequence:

```sql
-- For SQL steps
CREATE OR REPLACE TEMP VIEW cet_replay_target AS SELECT '<replay_id>' AS replay_id;
```

Then run:

1. `jobs/late_event_replay_prod.py`
2. `jobs/recompute_trends_prod.py`
3. `jobs/retract_and_upsert_v2.sql`
4. `jobs/replay_validation.sql`

## Production SLOs

Initial SLOs:

```text
p95 batch duration < 30 seconds
p99 batch duration < 60 seconds
DLQ/error ratio < 0.5%
paths_truncated = 0
states_truncated = 0
```

Any truncation is a correctness risk and should page.
