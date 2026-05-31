# 0xDSI CET Main-Ready v5

v5 closes the remaining gaps required to turn CET from a strong runtime into a
production product surface for 0xDSI.

## Added

- packaged native library loading for wheels;
- main-ready Databricks notebook with Delta event buffer for cross-microbatch windows;
- operational metrics contract and emission helpers;
- Attack Chain Compiler;
- Temporal Security Knowledge Graph materialization helpers;
- counterfactual replay diff helpers;
- agentic investigation package API;
- OCSF/ECS/Sigma-like adapters;
- Databricks main-ready jobs;
- GitHub Actions main-ready CI;
- Makefile and release scripts;
- SBOM fallback generation.

## Still deployment-specific

Before declaring a customer environment production complete, run:

1. Databricks bundle validate/deploy in dev and prod.
2. Unity Catalog grants validation.
3. 1M/10M/100M event benchmark matrix.
4. Replay-storm drill.
5. Alert backend integration.
6. Signed release and final license decision.
