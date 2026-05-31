# CET Metrics Spec v2

Runtime metrics emitted by `notebooks/0xDSI_CET_Databricks_prod.py` into
`main.cet.cet_metrics_v2`.

Required dimensions:

- `stream_id`
- `batch_id`
- `query_id`
- `query_version`
- `partition_key`
- `metric_name`
- `metric_value`
- `created_at`

Required metric names:

- `batch_duration_seconds`
- `paths_found`
- `paths_truncated`
- `states_truncated`
- `temporal_rejects`
- `success_records`
- `error_records`

Recommended derived metrics:

- `cet_dead_letter_ratio = error_records / (success_records + error_records)`
- `cet_truncation_ratio = paths_truncated / greatest(paths_found, 1)`
- `cet_temporal_reject_rate = temporal_rejects / greatest(paths_found, 1)`
- `cet_hot_partition_paths = max(paths_found by partition_key)`
- `cet_replay_backlog_count = count(cet_impacted_windows_v2 not yet closed)`

## Native runtime metrics
- cet_native_threads_used
- cet_parallel_workers
- cet_mmap_allocations
- cet_mmap_bytes
- cet_paths_truncated
- cet_states_truncated
