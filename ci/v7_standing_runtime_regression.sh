#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python -m py_compile \
  bindings/python/standing/__init__.py \
  bindings/python/standing/runtime.py \
  bindings/python/standing/sinks.py \
  jobs/standing_runtime_replay_prod.py \
  notebooks/0xDSI_CET_Standing_Runtime.py
python tests/standing_runtime_test.py
python tests/relation_aware_standing_runtime_test.py
python tests/standing_sinks_test.py
python tests/v7_package_import_test.py
echo "v7 standing runtime regression passed"
