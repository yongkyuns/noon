#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

node scripts/build-python-worker.mjs

if [[ "${NOON_SKIP_WEB_PREFLIGHT:-0}" != "1" ]]; then
  # The #61 ownership inventory is an architecture ratchet, not passive documentation.
  # Validate both the checked-in inventory and the validator's ownership-class invariants
  # in the required web build so contradictory or growing Python semantic debt cannot land.
  PYTHONDONTWRITEBYTECODE=1 python3 scripts/semantic_ownership_check.py
  PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_semantic_ownership_check.py

  # Keep top-level browser modules and tests self-registering with the required web build.
  # This prevents a new JavaScript file or regression test from silently escaping syntax
  # validation / execution because this script's hand-maintained inventory was not updated.
  while IFS= read -r source; do
    node --check "$source"
  done < <(find web -maxdepth 1 -type f \( -name '*.js' -o -name '*.mjs' \) -print | sort)

  while IFS= read -r source; do
    node --check "$source"
  done < <(find web/js -type f -name '*.js' -print | sort)

  node --check scripts/build-python-worker.mjs
  node --check scripts/execution-worker-smoke.mjs
  node --check scripts/execution-worker-host-smoke.mjs
  node --check scripts/retained-execution-worker-smoke.mjs
  node --check scripts/authoring-execution-router-smoke.mjs
  node --check scripts/authoring-execution-lifecycle-smoke.mjs
  node --check scripts/browser-smoke.mjs
  node --check scripts/browser-backend-visual-parity.mjs
  node --check scripts/browser-visual-parity-lib.mjs
  node --check scripts/manim-raster-differential.mjs
  node --check scripts/manim-seek-playback-raster.mjs
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
  node --check scripts/host-callback-perf.mjs
  node --check scripts/deterministic-replay-smoke.mjs
  node --check scripts/cross-language-parity.mjs
  node --check scripts/manim-compat-smoke.mjs
  node --check scripts/manim-tutorial-smoke.mjs
  node --check scripts/playground-layout-smoke.mjs
  node --check scripts/composition-authoring-smoke.mjs
  node --check scripts/reactive-authoring-smoke.mjs
  node --check scripts/reactive-runtime-smoke.mjs
  node --check scripts/native-input-smoke.mjs
  node --check scripts/updater-callback-smoke.mjs
  node --test scripts/browser-visual-parity-lib.test.mjs
  node --test scripts/manim-reference-inventory.test.mjs
  node --test scripts/manim-reference-ledger.test.mjs
  node --test scripts/manim-reference-coverage.test.mjs
  node --test scripts/manim-reference-classification-lock.test.mjs

  for test_file in web/*.test.mjs; do
    node --test "$test_file"
  done

  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    web/python/_manim_compat.py \
    web/python/_manim_typst.py \
    web/python/_manim_rate_functions.py \
    web/python/_manim_phase_b.py \
    web/python/_manim_shared_geometry.py \
    web/python/_manim_animation_options.py \
    web/python/_manim_animate.py \
    web/python/_manim_rotate.py \
    web/python/_manim_composition.py \
    web/python/_manim_frame_sampling.py \
    web/python/_manim_lifecycle.py \
    web/python/_manim_growing.py \
    web/python/_manim_draw_border_then_fill.py \
    web/python/_manim_reactive.py \
    web/python/_manim_updaters.py
  PYTHONDONTWRITEBYTECODE=1 python3 -m compileall -q web/python/examples
  PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'

  if [[ "${NOON_SKIP_PLAYGROUND_TEST:-0}" != "1" ]]; then
    cargo test -p noon-web --test playground_examples
  fi
fi

if [[ "${NOON_WEB_PREFLIGHT_ONLY:-0}" == "1" ]]; then
  exit 0
fi

wasm_pack_args=(build crates/noon-web --target web --out-dir ../../web/pkg --release)
if [[ "${NOON_WASM_SKIP_OPT:-0}" == "1" ]]; then
  wasm_pack_args+=(--no-opt)
fi
wasm-pack "${wasm_pack_args[@]}"
node scripts/check-web-package.mjs
