#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

status=0
for path in web/perf-profile.js web/scene-perf.js; do
  if [[ ! -f "$path" ]]; then
    printf 'active perf frontend ratchet: required migrated performance page is missing: %s\n' "$path" >&2
    status=1
    continue
  fi

  matches="$(
    grep -nE '(^|[^[:alnum:]_])(NoonCanvasPlayer|demoSceneJson)([^[:alnum:]_]|$)' "$path" || true
  )"
  if [[ -n "$matches" ]]; then
    printf 'active perf frontend ratchet: deleted browser frontend returned in %s\n' "$path" >&2
    printf '%s\n' "$matches" >&2
    status=1
  fi
done

if (( status != 0 )); then
  cat >&2 <<'EOF'

The workflow-backed performance profiles were migrated to the split execution
engine / renderer boundary by #1011 and #1012. They must not regain the deleted
NoonCanvasPlayer/demoSceneJson frontend. Keep EngineScenePlayer as the temporary
A4 transport seam until its own deletion lands. See #959/A4 and #961/A6.8.
EOF
  exit 1
fi

echo "active performance frontend absence ratchet passed"
