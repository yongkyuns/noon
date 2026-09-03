#!/usr/bin/env bash
set -euo pipefail

ROOT="${NOON_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

found=0

check_layer() {
  manifest="$1"
  layer="$2"
  shift 2

  if [[ ! -f "$ROOT/$manifest" ]]; then
    echo "layer dependency ratchet: missing manifest for $layer: $manifest" >&2
    exit 2
  fi

  dep=""
  for dep in "$@"; do
    matches="$(
      grep -En \
        -e "^[[:space:]]*(\"${dep}\"|'${dep}'|${dep})[[:space:]]*=" \
        -e "^[[:space:]]*\\[[^]]*dependencies\\.(\"${dep}\"|'${dep}'|${dep})\\][[:space:]]*$" \
        "$ROOT/$manifest" || true
    )"
    if [[ -n "$matches" ]]; then
      printf '%s\n' "$matches" >&2
      echo "layer dependency ratchet: $layer must not depend on $dep" >&2
      found=1
    fi
  done
}

# Dependency arrows point down the engine stack. Lower layers must not import
# frontend/platform layers or later engine authorities merely for convenience.
check_layer \
  "crates/noon-core/Cargo.toml" \
  "Semantic Scene / noon-core" \
  noon-compile noon-runtime noon-render-wgpu noon-web noon

check_layer \
  "crates/noon-compile/Cargo.toml" \
  "Execution Plan compiler / noon-compile" \
  noon-runtime noon-render-wgpu noon-web noon

check_layer \
  "crates/noon-runtime/Cargo.toml" \
  "Runtime / noon-runtime" \
  noon-render-wgpu noon-web noon

check_layer \
  "crates/noon-render-wgpu/Cargo.toml" \
  "Renderer / noon-render-wgpu" \
  noon-web noon

if (( found != 0 )); then
  cat >&2 <<'EOF'

Phase A layer dependency direction was violated.
Keep Semantic Scene, lowering/compiler, Runtime, Renderer, and platform/frontend
ownership one-way. Move shared data/behavior to its owning lower layer instead of
adding an upward dependency. See #953, #960 A5 and #961 A6.8.
EOF
  exit 1
fi

echo "architecture layer dependency ratchet passed"
