# 0xDSI-CET (C Engine + Spark Runtime)

This repository contains a native C event-trend engine and a Databricks/Spark runtime wrapper that performs partitioned event-trend detection in streaming microbatches.

## How the system works

### 1) Input model
The runtime expects a Delta input table (`cet_events`) with event identifiers, partition keys, event types, and event-time columns. The Spark job validates required columns against `contracts/spark_input_schema_v1.json` before processing.

### 2) Native engine boundary
The C engine exports a stable API in `c_engine/include/cet.h`.
- Query parsing (`cet_parse_query`) converts sequence definitions into internal query structs.
- Graph construction APIs (`cet_graph_add_vertex`, `cet_graph_add_edge`) build per-partition event DAGs.
- Execution APIs (`cet_execute_mcet`, `cet_execute_tcet`, `cet_execute_hcet`) evaluate patterns over the graph.
- Optimizer/window APIs support graphlet detection, planning, and overlapping-window primitives.

A Python ctypes bridge (`bindings/python/bridge.py`) loads `liboxdsi_cet.so`, optionally verifies checksum integrity (`OXDSI_CET_SHA256`), marshals data into C structs, and calls execution entry points.

### 3) Streaming execution lifecycle
`notebooks/0xDSI_CET_Databricks.py` orchestrates runtime execution:
1. Validate schema contract.
2. Read streaming microbatch from `cet_events`.
3. Process in distributed partition context (`mapPartitions`) to avoid driver `collect` bottlenecks.
4. Build in-batch event graph per partition key.
5. Execute H-CET via native bridge.
6. Emit deterministic trend IDs (`query_version + partition_key + path`).
7. Write complete trends with idempotent Delta `MERGE`.
8. Write metrics and dead-letter diagnostics.

### 4) Reliability model
- Exactly-once-ish writes are achieved with deterministic IDs + Delta `MERGE`.
- Dead-letter sink captures partition-level failures without stopping all partitions.
- Checkpointed `foreachBatch` enables stream restart continuation.
- Replay/recompute/retract scaffolds under `jobs/` support late-event correction workflows.

### 5) Validation and quality gates
- Native unit tests run through CTest.
- Bridge integration smoke validates shared-library invocation.
- Fault-injection test verifies checksum failure behavior.
- Perf gate enforces benchmark thresholds from `contracts/perf_thresholds_v1.json`.
- Golden/property tests validate deterministic behavior and semantics.
- Short soak loop checks repeated stability.

## Repository map
- `c_engine/`: native implementation and C tests
- `bindings/python/`: ctypes bridge
- `notebooks/`: Spark streaming runtime
- `jobs/`: replay/recompute/retract/cache persistence scaffolds
- `contracts/`: schema, perf, SLO, benchmark, and versioning contracts
- `ci/`: local CI scripts used by workflow
- `.github/workflows/ci.yml`: CI pipeline
- `dashboards/`: metrics + alerting specs
- `runbooks/`: operations + canary procedures

## Build and test
```bash
cmake -S . -B build
cmake --build build
ctest --test-dir build --output-on-failure
```

## Extended checks
```bash
./ci/integration_smoke.sh
./ci/fault_injection.sh
./ci/perf_gate.sh
PYTHONPATH=. python tests/golden_replay_test.py
PYTHONPATH=. python tests/property_semantics_test.py
```

## Deployment notes
- Build `liboxdsi_cet.so` on cluster image/init.
- Set `OXDSI_CET_SHA256` for runtime integrity checks.
- Provision Delta tables referenced by notebook (`cet_events`, `cet_complete_trends`, `cet_metrics`, `cet_dead_letter`).
- Configure replay/recompute jobs as Databricks workflows.
