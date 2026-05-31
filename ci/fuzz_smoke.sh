#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python tests/dsl_fuzz_smoke_test.py
echo "fuzz smoke passed"
