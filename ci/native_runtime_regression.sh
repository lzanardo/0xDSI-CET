#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.

cmake -S . -B build -DCET_ENABLE_PTHREADS=ON -DCET_ENABLE_MMAP_ARENA=ON
cmake --build build
python tests/parallel_equivalence_test.py
python tests/mmap_runtime_test.py
python benchmarks/parallel_scaling.py --events 2000 --threads 2 --mmap-workspace-bytes $((64 * 1024 * 1024))

echo "native runtime regression passed"
