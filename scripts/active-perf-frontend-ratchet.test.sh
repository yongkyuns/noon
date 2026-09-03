#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/active-perf-frontend-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/web"
cp "$RATCHET" "$TMP/scripts/active-perf-frontend-ratchet.sh"

cat > "$TMP/web/perf-profile.js" <<'EOF'
import { EngineScenePlayer, ExecutionCanvasRenderer } from "./pkg/noon_web.js";
EOF
cat > "$TMP/web/scene-perf.js" <<'EOF'
import { EngineScenePlayer, ExecutionCanvasRenderer } from "./pkg/noon_web.js";
EOF

(
  cd "$TMP"
  bash scripts/active-perf-frontend-ratchet.sh >/dev/null
)

expect_rejected() {
  label="$1"
  if (
    cd "$TMP"
    bash scripts/active-perf-frontend-ratchet.sh >/dev/null 2>&1
  ); then
    echo "active perf frontend ratchet test failed: accepted $label" >&2
    exit 1
  fi
}

cat > "$TMP/web/perf-profile.js" <<'EOF'
import { NoonCanvasPlayer } from "./pkg/noon_web.js";
EOF
expect_rejected 'NoonCanvasPlayer in perf-profile.js'

cat > "$TMP/web/perf-profile.js" <<'EOF'
import { EngineScenePlayer, ExecutionCanvasRenderer } from "./pkg/noon_web.js";
EOF
cat > "$TMP/web/scene-perf.js" <<'EOF'
const source = demoSceneJson();
EOF
expect_rejected 'demoSceneJson in scene-perf.js'

rm "$TMP/web/scene-perf.js"
expect_rejected 'missing migrated scene-perf.js'

echo "active performance frontend absence ratchet self-test passed"
