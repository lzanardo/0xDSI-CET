#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
rm -rf build-sanitize
cmake -S . -B build-sanitize -DCET_ENABLE_PTHREADS=ON -DCET_ENABLE_MMAP_ARENA=ON -DCET_ENABLE_SANITIZERS=ON
cmake --build build-sanitize
ctest --test-dir build-sanitize --output-on-failure
echo "sanitizer smoke passed"
