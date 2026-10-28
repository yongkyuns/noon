#!/usr/bin/env bash

set -euo pipefail

# Python validation must not add interpreter caches to the browser package.
export PYTHONDONTWRITEBYTECODE=1

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

skip_web_preflight="${NOON_SKIP_WEB_PREFLIGHT:-0}"
web_preflight_only="${NOON_WEB_PREFLIGHT_ONLY:-0}"
parallel_worker=0

# The package-only path has no preflight dependency on the generated Python worker,
# so overlap worker generation with the much longer WASM build. Full/preflight builds
# keep the historical synchronous order because their checks consume worker artifacts.
if [[ "$skip_web_preflight" == "1" && "$web_preflight_only" != "1" ]]; then
  parallel_worker=1
else
  node scripts/build-python-worker.mjs
fi

if [[ "$skip_web_preflight" != "1" ]]; then
  # The #61 ownership inventory is an architecture ratchet, not passive documentation.
  # Validate both the checked-in inventory and the validator's ownership-class invariants
  # in the required web build so contradictory or growing Python semantic debt cannot land.
  python3 scripts/semantic_ownership_check.py
  python3 scripts/test_semantic_ownership_check.py
  python3 scripts/test_manim_differential.py

  # Keep top-level browser modules and tests self-registering with the required web build.
  # This prevents a new JavaScript file or regression test from silently escaping syntax
  # validation / execution because this script's hand-maintained inventory was not updated.
  while IFS= read -r source; do
    node --check "$source"
  done < <(find web -maxdepth 1 -type f \( -name '*.js' -o -name '*.mjs' \) -print | sort)

  node --check scripts/build-python-worker.mjs
  node --test scripts/worker-composite-rows.test.mjs
  node --check scripts/build-runtime-identity.mjs
  node --check scripts/execution-worker-smoke.mjs
  node --check scripts/glow-worker-qualification.mjs
  node --check scripts/gaussian-blur-qualification.mjs
  node --test scripts/gaussian-blur-reference.test.mjs
  node --test scripts/dissolve-mask-reference.test.mjs
  node --check scripts/scope-opacity-qualification.mjs
  node --check scripts/effect-restore-qualification.mjs
  node --check scripts/pixelate-qualification.mjs
  node --check scripts/glass-lens-qualification.mjs
  node --check scripts/glass-lens-recovery-qualification.mjs
  node --check scripts/linear-gradient-qualification.mjs
  node --check scripts/radial-gradient-qualification.mjs
  node --check scripts/noise-paint-qualification.mjs
  node --check scripts/ordered-effects-qualification.mjs
  node --check scripts/composed-effects-qualification.mjs
  node --check scripts/noise-dissolve-qualification.mjs
  node --check scripts/dissolve-qualification.mjs
  node --check scripts/noise-paint-reference.mjs
  node --test scripts/noise-paint-reference.test.mjs
  node --check scripts/hatch-paint-qualification.mjs
  node --check scripts/hatch-paint-reference.mjs
  node --test scripts/hatch-paint-reference.test.mjs
  node --check scripts/image-fill-qualification.mjs
  node --check scripts/glow-worker-reference.mjs
  node --test scripts/glow-worker-reference.test.mjs
  node --check scripts/execution-worker-host-smoke.mjs
  node --check scripts/authoring-execution-router-smoke.mjs
  node --check scripts/browser-smoke.mjs
  node --check scripts/background-audio-smoke.mjs
  node --check scripts/browser-audio-observation.mjs
  node --check scripts/python-background-audio-smoke.mjs
  node --check scripts/export-python-scene.mjs
  node --check scripts/browser-backend-visual-parity.mjs
  node --check scripts/browser-visual-parity-lib.mjs
  node --check scripts/manim-raster-differential.mjs
  node --check scripts/shared-playback-raster.mjs
  node --check scripts/manim-typst-authoring-smoke.mjs
  node --check scripts/manim-reference-inventory.mjs
  node --check scripts/manim-reference-ledger.mjs
  node --check scripts/manim-reference-coverage.mjs
  node --check scripts/manim-reference-classification-lock.mjs
  node --check scripts/perf-profile.mjs
  node --check scripts/authoring-perf.mjs
  node --check scripts/perf-device-run.mjs
  node --check scripts/perf-compare.mjs
  node --check scripts/perf-corpus.mjs
  node --check scripts/stage-special-camera-raster.mjs
  node --check scripts/host-callback-perf.mjs
  node --check scripts/playground-cold-start.mjs
  node --check scripts/playground-cold-start-memory.mjs
  node --check scripts/deterministic-replay-smoke.mjs
  node --check scripts/browser-test-server.mjs
  node --test scripts/browser-test-server.test.mjs
  node --check scripts/cross-language-parity.mjs
  node --check scripts/typed-authoring-errors-smoke.mjs
  node --check scripts/manim-compat-smoke.mjs
  node --check scripts/manim-tutorial-smoke.mjs
  node --check scripts/python-editor-input-smoke.mjs
  node --check scripts/playground-layout-smoke.mjs
  node --check scripts/playground-product-e2e.mjs
  node --check scripts/playground-product-fps.mjs
  node --check scripts/composition-authoring-smoke.mjs
  node --check scripts/reactive-authoring-smoke.mjs
  node --check scripts/shared-authoring-smoke.mjs
  node --check scripts/shared-authoring-pyodide-contract.mjs
  node --check scripts/paired-authoring-qualification.mjs
  node --check scripts/object-membership-qualification.mjs
  node --test scripts/manim-family-points-reference.test.mjs
  node --check scripts/family-root-points-qualification.mjs
  node --check scripts/family-member-queries-qualification.mjs
  node --test scripts/manim-family-iteration-reference.test.mjs
  node --test scripts/manim-family-slices-reference.test.mjs
  node --test scripts/manim-object-queries-reference.test.mjs
  node --test scripts/manim-object-members-reference.test.mjs
  node --test scripts/manim-path-keyword-reference.test.mjs
  node --test scripts/manim-point-list-insertion-reference.test.mjs
  node --test scripts/manim-scalar-path-precision-reference.test.mjs
  node --test scripts/manim-point-append-reference.test.mjs
  node --test scripts/manim-scalar-xyz-reference.test.mjs
  node --test scripts/manim-depth-affine-reference.test.mjs
  node --test scripts/manim-fitting-xyz-reference.test.mjs
  node --test scripts/manim-rotated-xyz-reference.test.mjs
  node --test scripts/manim-scale-xyz-reference.test.mjs
  node --test scripts/manim-xyz-queries-reference.test.mjs
  node --check scripts/xyz-queries-qualification.mjs
  node --check scripts/point-promotion-qualification.mjs
  node --test scripts/manim-query-hooks-reference.test.mjs
  node --check scripts/table-qualification.mjs
  node --check scripts/table-position-checks.mjs
  node --check scripts/table-group-checks.mjs
  node --check scripts/table-mutation-checks.mjs
  node --check scripts/code-authoring-pyodide-contract.mjs
  node --check scripts/code-qualification.mjs
  node --check scripts/polynomial-rate-qualification.mjs
  node --check scripts/polynomial-easing-qualification.mjs
  node --check scripts/elastic-bounce-easing-qualification.mjs
  node --check scripts/lingering-qualification.mjs
  node --check scripts/cubic-bezier-qualification.mjs
  node --check scripts/special-camera-qualification.mjs
  node --check scripts/plotting-qualification.mjs
  node --check scripts/blur-qualification.mjs
  node --check scripts/shadow-qualification.mjs
  node --check scripts/playground-gallery-runtime-smoke.mjs
  node --check scripts/retained-dynamic-stress-perf.mjs
  node --check scripts/native-input-smoke.mjs
  node --check scripts/updater-callback-smoke.mjs
  node --check scripts/manim-host-updater-diagnostics.mjs
  node --check scripts/pr-risk-classifier.mjs
  node --test scripts/build-runtime-identity.test.mjs
  node --test scripts/pyodide-resource-cache.test.mjs
  node --test scripts/direct-program-sample.test.mjs
  node --test scripts/playground-cold-start-memory.test.mjs
  node --test scripts/playground-browser-failure-evidence.test.mjs
  node --test scripts/webgpu-device-capture.test.mjs
  node --test scripts/manim-raster-support.test.mjs
  node --test scripts/live-coordinate-checks.test.mjs
  node --test scripts/browser-visual-parity-lib.test.mjs
  node --test scripts/manim-reference-inventory.test.mjs
  node --test scripts/manim-reference-ledger.test.mjs
  node --test scripts/manim-reference-coverage.test.mjs
  node --test scripts/manim-reference-classification-lock.test.mjs
  node --test scripts/pr-risk-classifier.test.mjs
  node --test scripts/retained-dynamic-stress-perf-lib.test.mjs
  node --test scripts/perf-corpus-budget.test.mjs
  node --test scripts/perf-corpus-provenance.test.mjs
  node --test scripts/perf-compare.test.mjs
  node --test scripts/retained-typst-workflow-policy.test.mjs
  node --test scripts/playground-product-fps.test.mjs

  for test_file in web/*.test.mjs; do
    node --test "$test_file"
  done

  python3 - <<'PYTHON'
from pathlib import Path
for source in Path("web/python").rglob("*.py"):
    compile(source.read_bytes(), str(source), "exec")
PYTHON
  python3 -m unittest discover -s web/python -p 'test_*.py'

fi

if [[ "$web_preflight_only" == "1" ]]; then
  exit 0
fi

wasm_profile="${NOON_WASM_PROFILE:-release}"
case "$wasm_profile" in
  dev|release) ;;
  *)
    echo "unsupported NOON_WASM_PROFILE: $wasm_profile (expected dev or release)" >&2
    exit 2
    ;;
esac

# Validate before wasm-pack's own metadata lookup can rewrite a stale lock.
cargo metadata --locked --format-version 1 > /dev/null

wasm_pack_args=(build crates/noon-web --target web --out-dir ../../web/pkg "--$wasm_profile")
if [[ "${NOON_WASM_SKIP_OPT:-0}" == "1" ]]; then
  wasm_pack_args+=(--no-opt)
fi

if [[ "${NOON_RENDERER_SMOKE:-0}" == "1" ]]; then
  wasm_pack_args+=(--features renderer-smoke)
fi

if [[ "${NOON_REPLAY_SMOKE:-0}" == "1" ]]; then
  wasm_pack_args+=(--features replay-smoke)
fi

# CI and production must use the reviewed workspace dependency resolution.
# A separator here becomes a Cargo argument after any --features option.
wasm_pack_args+=(--locked)

worker_pid=""
if (( parallel_worker == 1 )); then
  node scripts/build-python-worker.mjs &
  worker_pid=$!
fi

wasm_status=0
wasm-pack "${wasm_pack_args[@]}" || wasm_status=$?

worker_status=0
if [[ -n "$worker_pid" ]]; then
  wait "$worker_pid" || worker_status=$?
fi

if (( wasm_status != 0 )); then
  exit "$wasm_status"
fi
if (( worker_status != 0 )); then
  exit "$worker_status"
fi

node scripts/build-runtime-identity.mjs
node scripts/check-web-package.mjs
