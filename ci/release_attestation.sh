#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH=.
cmake -S . -B build
cmake --build build
sha256sum build/liboxdsi_cet.so > build/liboxdsi_cet.so.sha256
cat build/liboxdsi_cet.so.sha256
