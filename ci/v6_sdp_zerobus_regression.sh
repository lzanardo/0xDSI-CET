#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
python -m py_compile \
  bindings/python/connectors/base.py \
  bindings/python/connectors/zerobus.py \
  bindings/python/connectors/sdp.py \
  jobs/zerobus_to_delta_ingest_prod.py \
  jobs/sdp_cet_runtime_bridge_prod.py \
  notebooks/0xDSI_CET_SDP.py
python tests/zerobus_connector_test.py
python tests/sdp_pipeline_compile_test.py
echo "v6 SDP/ZeroBus regression passed"
