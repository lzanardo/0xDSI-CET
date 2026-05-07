# Benchmark Harness Plan

1. Generate synthetic event streams per partition.
2. Feed windows into C API (`cet_graph_add_*`, `cet_execute_*`).
3. Compare M-CET/T-CET/H-CET latency and RSS.
4. Export metrics to `cet_metrics` Delta table via Databricks job.
