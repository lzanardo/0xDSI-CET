# 0xDSI-CET Native Runtime v3

This patch adds optional native runtime controls to the CET C engine:

- POSIX-thread parallel H-CET for large partitions, offline replay, and backfill.
- mmap-backed native workspace for pthread worker metadata and per-thread result buffers.
- Runtime diagnostics for thread usage, workspace allocation, and mmap use.
- Conservative Spark defaults to avoid Databricks executor oversubscription.

## Important default

Structured Streaming should normally use:

```bash
CET_NATIVE_THREADS=1
```

Spark already parallelizes by executor/task/partition. If every Spark task opens multiple native threads, you can accidentally create `spark_tasks * native_threads` CPU contenders. Use `native_threads > 1` primarily for offline recompute/backfill, standalone benchmarks, or explicitly sized executor pools.

## Runtime knobs

Environment variables for native execution:

```bash
export CET_NATIVE_THREADS=1
export CET_ENABLE_MMAP_ARENA=0
export CET_MMAP_WORKSPACE_BYTES=67108864
export CET_MADVISE_HUGEPAGE=0
```

Databricks/Spark confs used by `notebooks/0xDSI_CET_Databricks_prod.py`:

```python
spark.conf.set('oxdsi.cet.nativeThreads', '1')
spark.conf.set('oxdsi.cet.enableMmapArena', 'false')
spark.conf.set('oxdsi.cet.mmapArenaBytes', str(256 * 1024 * 1024))
```

## What mmap covers in v3

The v3 mmap support backs the native runtime workspace used for pthread worker metadata and per-thread result buffers. The existing core traversal allocations for adjacency indexes and BFS state queues still use heap allocation inside `algorithms.c`. This is intentional for a safe incremental upstream patch.

A future v4 can move adjacency and BFS state into the same workspace/arena once equivalence and memory-pressure tests are expanded.

## Parallel strategy

The parallel runtime shards traversal by vertex range, runs each shard in a private worker result buffer, then merges results deterministically by worker order. This avoids locks in the hot path.

Validation included:

```bash
./ci/production_regression.sh
./ci/native_runtime_regression.sh
```

Expected benchmark output includes fields like:

```text
threads_used=2 parallel=True mmap=True workspace_bytes=67108864
```

## Kernel module decision

Do not move CET matching into a kernel module. Keep kernel/eBPF/XDP for telemetry collection and pre-filtering only; keep graph traversal, replay, query semantics, and Delta/Spark integration in userspace.
