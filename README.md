# 0xDSI-CET (C Engine + Spark Runtime)

0xDSI-CET is a native C implementation of Complete Event Trend Detection (CET) with a Python bridge and Databricks Structured Streaming runtime.

This repository operationalizes the core ideas from the CET literature in a production-oriented shape:
- graph-based trend representation,
- multiple execution modes (M-CET / T-CET / H-CET),
- sliding-window + graphlet planning concepts,
- runtime controls for reliability, observability, and deployment safety.

---

## 1) End-to-end architecture

### 1.1 Ingestion and contracts
Input arrives in Delta table `cet_events` and is validated against `contracts/spark_input_schema_v1.json`.
Required fields are expected to include event identity, partition key, event type, and event-time columns.

### 1.2 Native execution boundary
The C engine API (in `c_engine/include/cet.h`) provides:
- query parsing (`cet_parse_query`),
- graph building (`cet_graph_add_vertex`, `cet_graph_add_edge`),
- execution (`cet_execute_mcet`, `cet_execute_tcet`, `cet_execute_hcet`),
- optimizer/window helpers.

The Python bridge (`bindings/python/bridge.py`) marshals Python tuples into C structs and calls native executors. Optional integrity check is enforced by `OXDSI_CET_SHA256`.

### 1.3 Streaming orchestration
`notebooks/0xDSI_CET_Databricks.py` runs `foreachBatch`:
1. validates schema contract,
2. executes partition-side processing via `mapPartitions`,
3. constructs partition-local event graphs,
4. calls native H-CET,
5. writes trends by deterministic `trend_id` using Delta `MERGE`,
6. writes metrics + dead-letter diagnostics.

### 1.4 Reliability controls
- deterministic IDs (`query_version + partition + path`) for idempotent writes,
- DLQ table for partition-level failures,
- checkpointed stream for restart continuity,
- replay/recompute/retract job scaffolds for late-event correction.

---

## 2) Execution model details

## 2.1 M-CET (memory-oriented)
Depth-first traversal over adjacency index with sequence and `WITHIN` constraints.

## 2.2 T-CET (time-oriented)
Breadth-first frontier traversal with partial-cache touchpoints for reusable subpaths.

## 2.3 H-CET (hybrid)
BFS prefix seeding + DFS suffix expansion for mixed memory/latency behavior.

## 2.4 Skip-till-any-match and Kleene
The parser supports sequence tokens with Kleene-plus (e.g., `A+`), and traversal logic supports skip-till-any-match behavior for non-contiguous matching paths.

---

## 3) Paper-style pattern examples (implemented in this repo)

The CET use-cases typically model fraud/security/market trends as event sequences in windows. This repo includes representative examples under `examples/scenarios.md` and notebook query usage.

### 3.1 Check-kiting style trend
`Deposit+, Withdrawal, Transfer` within time window and sliding interval.

### 3.2 Stock trend pattern
`Tick+, RallySignal` where repeated ticks represent monotonic trend phases.

### 3.3 Security escalation pattern
`AuthFail+, PrivEsc, DataAccess` (used in notebook runtime example).

These are implemented as CET sequence queries through `parse_query(...)` and executed through H-CET in streaming microbatches.

---

## 4) CI / quality gates

CI workflow runs:
- native build + CTest,
- integration smoke (Python bridge -> shared library),
- checksum fault-injection path,
- perf gate against threshold contract,
- property semantics tests,
- golden replay tests,
- migration/version checks,
- release checksum attestation,
- short soak loop.

Key scripts:
- `ci/integration_smoke.sh`
- `ci/fault_injection.sh`
- `ci/perf_gate.sh`
- `ci/migration_check.sh`
- `ci/release_attestation.sh`
- `ci/soak_test.sh`

---

## 5) Contracts and operations

### 5.1 Contracts
- `contracts/spark_input_schema_v1.json`
- `contracts/cet_output_schema_v1.json`
- `contracts/perf_thresholds_v1.json`
- `contracts/slo_targets.json`
- `contracts/query_version_policy_v1.json`
- `contracts/benchmark_matrix_v1.json`

### 5.2 Dashboards and alerts
- `dashboards/metrics_spec.md`
- `dashboards/alert_rules.yaml`
- `dashboards/deploy_alerts.py`

### 5.3 Replay and recompute jobs
- `jobs/late_event_replay.py`
- `jobs/recompute_trends.py`
- `jobs/retract_and_upsert.sql`
- `jobs/persist_partial_cache.py`
- `jobs/calibrate_cost_model.py`

---

## 6) Build and run

```bash
cmake -S . -B build
cmake --build build
ctest --test-dir build --output-on-failure
```

Extended checks:

```bash
./ci/integration_smoke.sh
./ci/fault_injection.sh
./ci/perf_gate.sh
PYTHONPATH=. python tests/golden_replay_test.py
PYTHONPATH=. python tests/property_semantics_test.py
```

---

## 7) Current status

This repository is production-oriented and includes substantial runtime/CI/ops scaffolding. 
For full enterprise closure, continue with long-horizon benchmark publication, in-cluster integration stress tests, and fully automated replay correctness orchestration.
