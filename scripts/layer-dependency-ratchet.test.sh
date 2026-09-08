#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Default: all tests, including real offline Cargo fixture workspaces. Missing
# Cargo is a failure, never a skipped test. Named unittest classes may be passed
# for focused iteration; that subset does not qualify the complete scanner.
exec python3 "$ROOT/scripts/layer_dependency_ratchet_test.py" "$@"
