#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

manifest="crates/noon-render-wgpu/Cargo.toml"
source_root="crates/noon-render-wgpu/src"

# noon-render-wgpu is reusable retained rendering machinery. Window/canvas/event-loop
# and surface lifecycle dependencies belong to native/web platform integration.
forbidden_direct_deps='^[[:space:]]*(winit|raw-window-handle|web-sys|js-sys|wasm-bindgen)[[:space:]]*='

found=0
if grep -En "$forbidden_direct_deps" "$manifest"; then
  cat >&2 <<'EOF'
renderer host-boundary ratchet: noon-render-wgpu gained a platform-host dependency.
Window/browser lifecycle dependencies belong at the native/web platform edge (#960/#969),
not in the reusable renderer.
EOF
  found=1
fi

mapfile -t rust_sources < <(git ls-files "$source_root" | grep -E '\.rs$' || true)
if (( ${#rust_sources[@]} == 0 )); then
  echo "renderer host-boundary ratchet: no tracked Rust sources found under $source_root" >&2
  exit 2
fi

# wgpu device/queue/texture-view/encoder types are renderer machinery. Surface
# creation/configuration/acquisition/presentation is platform-host lifecycle and must
# remain outside this crate.
forbidden_source_api='winit::|web_sys::|js_sys::|wasm_bindgen::|wgpu::Surface([^A-Za-z0-9_]|$)|wgpu::SurfaceConfiguration([^A-Za-z0-9_]|$)|wgpu::SurfaceTexture([^A-Za-z0-9_]|$)|create_surface[[:space:]]*\(|get_current_texture[[:space:]]*\('

if grep -En "$forbidden_source_api" "${rust_sources[@]}"; then
  cat >&2 <<'EOF'
renderer host-boundary ratchet: noon-render-wgpu contains platform surface/window/browser lifecycle code.
Keep surface creation/configuration/frame acquisition/presentation in the native or web host and pass
renderer-owned GPU inputs through typed Rust APIs instead.
EOF
  found=1
fi

if (( found != 0 )); then
  exit 1
fi

echo "renderer platform-host boundary ratchet passed"
