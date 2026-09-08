#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Real Cargo fixtures are mandatory, not silently skipped on unprepared hosts.
command -v cargo >/dev/null || { echo 'layer ratchet self-test requires cargo' >&2; exit 2; }
exec python3 "$ROOT/scripts/layer_dependency_ratchet.test.py" "$@"
