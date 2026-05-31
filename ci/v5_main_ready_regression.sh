#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python -m py_compile \
  bindings/python/native_loader.py \
  bindings/python/metrics.py \
  bindings/python/attack_chain.py \
  bindings/python/temporal_kg.py \
  bindings/python/counterfactual.py \
  bindings/python/investigation.py \
  bindings/python/adapters/ocsf.py \
  bindings/python/adapters/ecs.py \
  bindings/python/adapters/sigma.py \
  jobs/materialize_temporal_kg_prod.py \
  jobs/counterfactual_replay_prod.py \
  notebooks/0xDSI_CET_Databricks_main.py
python tests/native_loader_test.py
python tests/attack_chain_compiler_test.py
python tests/temporal_kg_test.py
python tests/counterfactual_test.py
python tests/investigation_api_test.py
python tests/adapters_test.py
echo "v5 main-ready regression passed"
