#!/usr/bin/env bash
set -euo pipefail
noon_evidence_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
noon_repository="${NOON_REPO:-/workspace/scratch/a21b300760f5/noon}"
cd "$noon_repository"
# The captured wrapper ran as stdin, so its relative module imports use the checkout.
node --input-type=module < "$noon_evidence_dir/wrapper.mjs"
