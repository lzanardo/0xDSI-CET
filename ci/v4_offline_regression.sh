#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python -m py_compile \
  bindings/python/dsl.py \
  bindings/python/security_graph.py \
  bindings/python/query_registry.py \
  bindings/python/multi_query_runtime.py \
  bindings/python/risk.py \
  bindings/python/features.py \
  bindings/python/state.py \
  bindings/python/event_contracts.py \
  jobs/state_ttl_prod.py \
  jobs/replay_diff_prod.py \
  jobs/query_registry_migration.py \
  notebooks/0xDSI_CET_Databricks_v4.py \
  benchmarks/enterprise_workloads.py
python tests/dsl_compiler_test.py
python tests/security_graph_builder_test.py
python tests/multi_query_runtime_test.py
python tests/state_engine_test.py
python tests/risk_scoring_test.py
python tests/package_import_test.py
python benchmarks/enterprise_workloads.py --events 5000 --queries 2 --offline-only
echo "v4 offline regression passed"
