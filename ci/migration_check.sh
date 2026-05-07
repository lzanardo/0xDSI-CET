#!/usr/bin/env bash
set -euo pipefail
python - <<'PY'
import json
inp=json.load(open('contracts/spark_input_schema_v1.json'))
out=json.load(open('contracts/cet_output_schema_v1.json'))
assert inp['version'].startswith('v')
assert out['version'].startswith('v')
print('migration contract check passed', inp['version'], out['version'])
PY
