#!/usr/bin/env bash
set -euo pipefail
python -m pip install --upgrade pip build wheel setuptools >/dev/null
python -m build --wheel
ls -lh dist/*.whl
