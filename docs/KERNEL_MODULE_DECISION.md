# Kernel Module Decision for CET

Do not implement the CET matching engine as a Linux kernel module.

Recommended split:

```text
eBPF/XDP/AF_XDP/audit/ETW/cloud telemetry  -> collection and pre-filtering
userspace C CET engine                     -> graph matching, state, replay semantics
Spark/Delta/Databricks                     -> history, replay, governance, lineage
```

Rationale:

- CET graph traversal has dynamic memory, path explosion, query semantics, replay, and observability requirements that are safer in userspace.
- A userspace crash is recoverable; a kernel bug can panic the host.
- Delta/Spark replay and query governance do not belong in kernel space.
- For high-rate ingest, use eBPF/XDP/AF_XDP as a collector/pre-filter, not as the CET engine.

The v3 patch therefore uses POSIX threads and mmap in userspace instead of a kernel module.
