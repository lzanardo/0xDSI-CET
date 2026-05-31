#!/usr/bin/env bash
set -euo pipefail
mkdir -p dist
if command -v syft >/dev/null 2>&1; then
  syft dir:. -o cyclonedx-json > dist/sbom.cdx.json
else
  python - <<'PY'
from pathlib import Path
import hashlib, json
files=[]
for p in sorted(Path('.').rglob('*')):
    if p.is_file() and '.git' not in p.parts and 'build' not in p.parts and '__pycache__' not in p.parts:
        h=hashlib.sha256(p.read_bytes()).hexdigest()
        files.append({'path':str(p),'sha256':h,'bytes':p.stat().st_size})
Path('dist').mkdir(exist_ok=True)
Path('dist/sbom.files.json').write_text(json.dumps({'files':files},indent=2))
PY
fi
ls -lh dist/sbom*json
