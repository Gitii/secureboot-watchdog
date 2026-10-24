#!/bin/bash
# Actual helper/child/pipe tests; all hardware commands live in a temporary PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 tests/tpm_repair.py
