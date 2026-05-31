# CET v4 Metrics Spec

Required production metrics:

- `cet_v4_trends_emitted_total{query_id,query_version}`
- `cet_v4_batch_latency_seconds{p50,p95,p99}`
- `cet_v4_native_runtime_threads_used`
- `cet_v4_mmap_workspace_bytes`
- `cet_v4_path_truncation_total`
- `cet_v4_state_rows_active`
- `cet_v4_state_rows_expired_total`
- `cet_v4_replay_diff_stale_total`
- `cet_v4_replay_diff_new_total`
- `cet_v4_dead_letter_ratio`
- `cet_v4_partition_skew_ratio`
- `cet_v4_query_registry_enabled_count`
- `cet_v4_dsl_fallback_feature_count{feature}`
