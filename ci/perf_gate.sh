#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
cmake -S . -B build
cmake --build build
ctest --test-dir build --output-on-failure
OUT=$(python benchmarks/harness.py)
echo "$OUT"
python - <<'PY' "$OUT"
import json,re,sys
out=sys.argv[1]
thr=json.load(open('contracts/perf_thresholds_v1.json'))
m=re.search(r'paths=(\d+)\s+seconds=([0-9.]+)',out)
if not m:
    raise SystemExit('perf output parse failed')
paths=int(m.group(1)); sec=float(m.group(2))
assert paths>=thr['min_paths'], f'paths below threshold: {paths}'
assert sec<=thr['h_cet_max_seconds'], f'seconds above threshold: {sec}'
print('perf thresholds passed')
PY
