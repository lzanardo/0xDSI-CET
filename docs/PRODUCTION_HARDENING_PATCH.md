# 0xDSI-CET Production Hardening Patch

This patch closes the most important production gaps without rewriting the whole
engine category.

## What is fixed

### 1. Native engine diagnostics

`c_engine/include/cet.h` and `c_engine/src/algorithms.c` now expose
`cet_execute_*_ex(...)` variants with `cet_exec_stats_t`.

This prevents silent failure when the engine truncates paths/states and adds:

- `paths_truncated`
- `states_truncated`
- `temporal_rejects`
- `edge_window_rejects`
- `predicate_rejects`
- `overflow`
- `error`

The original `cet_execute_mcet`, `cet_execute_tcet`, and `cet_execute_hcet`
functions are still present for backward compatibility.

### 2. Stronger temporal semantics

Traversal now rejects:

- event time going backwards,
- current event earlier than the path start,
- current event outside `WITHIN`,
- endpoints outside the edge window when the edge provides a non-empty window.

### 3. Python bridge hardening

`bindings/python/bridge.py` now:

- checks event/edge limits before allocating native structs,
- checks return codes from `cet_graph_add_vertex` and `cet_graph_add_edge`,
- calls `cet_execute_hcet_ex` when available,
- returns native execution stats in `CETMatch.stats`,
- can raise on overflow via `raise_on_overflow=True`.

### 4. Stateful Structured Streaming runtime

`notebooks/0xDSI_CET_Databricks_prod.py` adds a production runtime v3:

- configurable durable checkpoint,
- driver-side query registry loading,
- no `spark.table(...)` inside executor `mapPartitions`,
- per-query event buffer state in Delta,
- trend detection across microbatch boundaries,
- rich trend output table,
- idempotent metrics and DLQ merges.

### 5. Replay closure

New replay jobs:

- `jobs/late_event_replay_prod.py`
- `jobs/recompute_trends_prod.py`
- `jobs/retract_and_upsert_v2.sql`
- `jobs/replay_validation.sql`

These implement the missing cycle:

```text
late/corrected events
→ impacted windows
→ recompute from source events
→ retract stale active trends
→ upsert recomputed active trends
→ validate replay
```

## What is intentionally not fully solved in this patch

This patch does not fully implement:

- a full CET DSL with `WHERE`, `ABSENCE`, bounded repetition, `OR`, entity joins,
- entity-resolution-based security graph construction,
- a real cost-based graphlet optimizer using observed cardinality/selectivity,
- signed release provenance / SBOM / SLSA,
- a distributed native state store inside the C engine.

The patch fixes the biggest production correctness and operations gaps first.

## Apply

From outside the repository:

```bash
unzip 0xDSI-CET-production-patch.zip
cd 0xDSI-CET-production-patch
./apply_overlay.sh /path/to/0xDSI-CET
```

Then in the repository:

```bash
./ci/production_regression.sh
```

## Databricks run order

1. Build and distribute `liboxdsi_cet.so` to the cluster.
2. Set durable checkpoint config:

```python
spark.conf.set("oxdsi.cet.checkpointLocation", "dbfs:/checkpoints/0xdsi/cet/prod-v2")
```

3. Run:

```python
notebooks/0xDSI_CET_Databricks_prod.py
```

4. For replay:

```text
jobs/late_event_replay_prod.py
jobs/recompute_trends_prod.py
jobs/retract_and_upsert_v2.sql
jobs/replay_validation.sql
```


## Native runtime v3 additions

- Added `CET_ENABLE_PTHREADS` and `CET_ENABLE_MMAP_ARENA` CMake switches.
- Added `cet_runtime_config_t`, `cet_runtime_stats_t`, and `cet_execute_hcet_parallel_ex`.
- Added optional mmap-backed workspace for parallel worker results.
- Added Python `run_hcet_parallel(...)`.
- Added `tests/parallel_equivalence_test.py` and `benchmarks/parallel_scaling.py`.
- Databricks runtime remains conservative: native threads default to 1 to avoid Spark executor oversubscription.


### 8. Native runtime v3

This patch adds optional userspace native acceleration rather than a kernel module:

- `CET_ENABLE_PTHREADS` CMake option for POSIX-thread H-CET sharding.
- `CET_ENABLE_MMAP_ARENA` CMake option for mmap-backed runtime workspace.
- `cet_execute_hcet_parallel_ex(...)` with runtime stats.
- Python `run_hcet_parallel(...)` and `run_hcet(..., native_threads=...)` convenience routing.
- `tests/parallel_equivalence_test.py`, `tests/mmap_runtime_test.py`, and `benchmarks/parallel_scaling.py`.

Keep native threads at 1 in Databricks streaming jobs unless executor sizing explicitly accounts for nested native parallelism. Use multiple native threads for replay, recompute, backfill, and standalone benchmarks.
