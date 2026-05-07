# 0xDSI-CET Architecture (Production)

## Engine + Algorithms
- Native C engine with M-CET/T-CET/H-CET executors.
- Graphlet optimizer with greedy and branch-and-bound planning.

## Streaming Runtime
- Distributed native execution in `mapPartitions`.
- Schema contract validation at batch boundary.
- Checkpointed `foreachBatch` orchestration.

## Reliability Controls
- Deterministic trend IDs + idempotent Delta MERGE.
- Partition-level error isolation with dead-letter sink.
- Library checksum verification in Python bridge.

## Operations
- Metrics sink (`paths_found`) and dead-letter diagnostics.
- CI perf gate script for build/test/benchmark smoke.
- Late-event replay SQL job scaffold.

## Next deep optimizations
- Replace fixed arrays with pooled arena/CSR adjacency.
- Persist T-CET partial-cache state across batches/windows.
- Calibrate cost model from observed runtime telemetry.

- Cost calibration job: `jobs/calibrate_cost_model.py`.
- Alert deployment hook: `dashboards/deploy_alerts.py`.
