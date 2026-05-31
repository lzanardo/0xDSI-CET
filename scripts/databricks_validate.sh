#!/usr/bin/env bash
set -euo pipefail
if ! command -v databricks >/dev/null 2>&1; then
  echo 'Databricks CLI not installed; skipping bundle validate' >&2
  exit 0
fi
databricks bundle validate -t dev
