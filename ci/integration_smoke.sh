#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python - <<'PY'
from bindings.python.bridge import CETBridge
b=CETBridge('build/liboxdsi_cet.so')
q=b.parse_query('q','A,B,C',60000,10000)
events=[(1,'p','A',1),(2,'p','B',2),(3,'p','C',3)]
edges=[(1,2,0,10),(2,3,0,10)]
out=b.run_hcet(q,events,edges)
assert out.paths, 'no paths'
print('integration smoke ok', out.paths)
PY
