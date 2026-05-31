# Production Readiness v4

v4 closes the remaining product gaps around:

- CET DSL and governed query registry.
- Security graph construction.
- Multi-query execution.
- Risk scoring and feature extraction.
- Databricks Asset Bundle packaging.
- State TTL/compaction jobs.
- Replay diff validation.
- CI, sanitizer, and fuzz smoke coverage.
- Supply-chain workflow hooks.

Still required before declaring a specific customer production deployment:

- run benchmark matrix on a real Databricks cluster;
- validate Unity Catalog permissions;
- configure production alert backend;
- produce signed artifacts/SBOM through the chosen CI system;
- execute disaster recovery and replay-storm drills.
