#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.

cmake -S . -B build -DCET_ENABLE_PTHREADS=ON -DCET_ENABLE_MMAP_ARENA=ON
cmake --build build
ctest --test-dir build --output-on-failure

python tests/golden_replay_test.py
python tests/property_semantics_test.py
python tests/temporal_semantics_test.py
python tests/parallel_equivalence_test.py
python tests/mmap_runtime_test.py
python -m py_compile \
  bindings/python/bridge.py \
  jobs/late_event_replay_prod.py \
  jobs/recompute_trends_prod.py \
  notebooks/0xDSI_CET_Databricks_prod.py \
  benchmarks/parallel_scaling.py

python benchmarks/parallel_scaling.py --events 2000 --threads 1 --mmap-workspace-bytes $((64 * 1024 * 1024))
python benchmarks/parallel_scaling.py --events 2000 --threads 2 --mmap-workspace-bytes $((64 * 1024 * 1024))

echo "production regression v3 passed"
