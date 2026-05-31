#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
cmake -S . -B build
cmake --build build
# bad checksum should fail
if OXDSI_CET_SHA256=deadbeef python - <<'PY'
from bindings.python.bridge import CETBridge
CETBridge('build/liboxdsi_cet.so')
PY
then
  echo 'expected checksum failure did not occur'; exit 1
fi
echo 'fault injection passed'
