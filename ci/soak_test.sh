#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
cmake -S . -B build
cmake --build build
for i in $(seq 1 20); do
  python benchmarks/harness.py >/tmp/bench_$i.txt
  cat /tmp/bench_$i.txt
  python tests/golden_replay_test.py >/tmp/golden_$i.txt
  cat /tmp/golden_$i.txt
done
echo 'soak test passed'
