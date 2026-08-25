#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

node --check web/main.js
node --check web/authoring-client.js
node --check web/python-worker.js
node --check web/native-inputs.js
node --check web/scene-pipeline-perf.mjs
node --check web/gpu-profile.js
node --check web/morph-profile.js
node --check web/browser-jank.js
node --check web/perf-profile.js
node --check web/perf-workloads.js
node --check web/browser-smoke.js
node --check scripts/browser-smoke.mjs
node --check scripts/perf-profile.mjs
node --check scripts/deterministic-replay-smoke.mjs
node --check scripts/manim-compat-smoke.mjs
node --check scripts/manim-tutorial-smoke.mjs
node --check scripts/composition-authoring-smoke.mjs
node --check scripts/reactive-authoring-smoke.mjs
node --check scripts/reactive-runtime-smoke.mjs
node --check scripts/native-input-smoke.mjs
node --check scripts/updater-callback-smoke.mjs
node --test web/authoring-client.test.mjs
node --test web/scene-identity.test.mjs
node --test web/frame-metrics.test.mjs
node --test web/browser-jank.test.mjs
node --test web/perf-workloads.test.mjs
node --test web/wire-contracts.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  web/python/_manim_compat.py \
  web/python/_manim_rate_functions.py \
  web/python/_manim_phase_b.py \
  web/python/_manim_animation_options.py \
  web/python/_manim_animate.py \
  web/python/_manim_composition.py \
  web/python/_manim_lifecycle.py \
  web/python/_manim_reactive.py \
  web/python/_manim_updaters.py
PYTHONDONTWRITEBYTECODE=1 python3 -m compileall -q web/python/examples
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'

if [[ "${NOON_SKIP_PLAYGROUND_TEST:-0}" != "1" ]]; then
  cargo test -p noon-web --test playground_examples
fi

wasm_pack_args=(build crates/noon-web --target web --out-dir ../../web/pkg --release)
if [[ "${NOON_WASM_SKIP_OPT:-0}" == "1" ]]; then
  wasm_pack_args+=(--no-opt)
fi
wasm-pack "${wasm_pack_args[@]}"
node scripts/check-web-package.mjs
