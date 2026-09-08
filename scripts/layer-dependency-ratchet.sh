#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOON_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
exec python3 "$SCRIPT_DIR/layer_dependency_ratchet.py" "$ROOT"
