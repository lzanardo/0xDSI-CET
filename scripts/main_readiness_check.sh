#!/usr/bin/env bash
set -euo pipefail
./ci/production_regression.sh
./ci/v4_offline_regression.sh
./ci/v5_main_ready_regression.sh
./ci/sanitizers.sh
./ci/fuzz_smoke.sh
python -m py_compile $(find bindings jobs benchmarks -name '*.py' -print)
python -m pip install --upgrade build >/dev/null
OXDSI_SKIP_NATIVE_BUILD=1 python -m build --wheel
./scripts/generate_sbom.sh
if git ls-files | grep -E '(^build/|__pycache__|\.pyc$|0xDSI-CET-.*patch|\.zip$|\.tar\.gz$)'; then
  echo 'tracked local artifact detected' >&2
  exit 1
fi
echo 'main readiness check passed'
