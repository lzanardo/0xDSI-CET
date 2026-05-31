#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
# Native/kernel checks from v3-v5 when a compiler is available.
if [ -x ./ci/production_regression.sh ]; then ./ci/production_regression.sh; fi
if [ -x ./ci/v4_offline_regression.sh ]; then ./ci/v4_offline_regression.sh; fi
if [ -x ./ci/v5_main_ready_regression.sh ]; then ./ci/v5_main_ready_regression.sh; fi
if [ -x ./ci/v6_sdp_zerobus_regression.sh ]; then ./ci/v6_sdp_zerobus_regression.sh; fi
./ci/v7_standing_runtime_regression.sh
echo "all-in-one v7 regression passed"
