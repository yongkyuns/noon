#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

wasm_profile="${NOON_WASM_PROFILE:-release}"
case "$wasm_profile" in
  dev|release) ;;
  *)
    echo "unsupported NOON_WASM_PROFILE: $wasm_profile (expected dev or release)" >&2
    exit 2
    ;;
esac

out_dir="${NOON_AUTHORING_WASM_OUT_DIR:-web/pkg-authoring}"
wasm_pack_args=(
  build crates/noon-web
  --target web
  --out-dir "../../${out_dir}"
  "--${wasm_profile}"
)
if [[ "${NOON_WASM_SKIP_OPT:-0}" == "1" ]]; then
  wasm_pack_args+=(--no-opt)
fi
# wasm-pack forwards Cargo feature flags and everything after them to cargo,
# so keep wasm-pack-owned flags (such as --no-opt) before this tail.
wasm_pack_args+=(--no-default-features)

wasm-pack "${wasm_pack_args[@]}"

wasm_file="${out_dir}/noon_web_bg.wasm"
js_file="${out_dir}/noon_web.js"
[[ -s "$wasm_file" ]] || {
  echo "authoring-only Noon WASM package is missing or empty: $wasm_file" >&2
  exit 1
}
[[ -s "$js_file" ]] || {
  echo "authoring-only Noon JS bindings are missing or empty: $js_file" >&2
  exit 1
}

for symbol in \
  WasmAuthoringStore \
  RetainedNativeTextAuthoringHandle \
  RetainedTypstAuthoringHandle \
  canonicalRetainedSceneSpecJson \
  manimAnnularSectorSnapshotJson \
  manimAnnulusSnapshotJson \
  manimDashedLineSnapshotJson \
  manimDotSnapshotJson \
  manimElbowSnapshotJson \
  manimRoundedRectangleSnapshotJson \
  manimSectorSnapshotJson \
  manimTriangleSnapshotJson \
  manimUnderlineSnapshotJson \
  resolveAnimationOptions \
  resolveCompositionSchedule \
  resolveLifecyclePlan \
  resolveUniformCompositionSchedule \
  validatePresenceTransition; do
  if ! grep -q "$symbol" "$js_file"; then
    echo "authoring-only package is missing required export: $symbol" >&2
    exit 1
  fi
done

for symbol in ExecutionCanvasRenderer RetainedExecutionCanvasRenderer RetainedTypstCanvasRenderer; do
  if grep -q "$symbol" "$js_file"; then
    echo "authoring-only package unexpectedly exposes renderer symbol: $symbol" >&2
    exit 1
  fi
done

node - "$wasm_file" <<'NODE'
const fs = require("node:fs");
const wasmPath = process.argv[2];
const bytes = fs.statSync(wasmPath).size;
process.stdout.write(`${wasmPath}: ${bytes} bytes\n`);
NODE
