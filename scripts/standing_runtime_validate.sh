#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
./ci/v7_standing_runtime_regression.sh
