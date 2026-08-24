#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

node --check web/main.js
node --check web/authoring-client.js
node --check web/python-worker.js
node --check web/scene-pipeline-perf.mjs
node --check web/gpu-profile.js
node --check web/morph-profile.js
node --check web/browser-smoke.js
node --check scripts/browser-smoke.mjs
node --check scripts/manim-compat-smoke.mjs
node --check scripts/reactive-authoring-smoke.mjs
node --check scripts/reactive-runtime-smoke.mjs
node --test web/authoring-client.test.mjs
node --test web/scene-identity.test.mjs
node --test web/frame-metrics.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  web/python/_manim_compat.py \
  web/python/_manim_phase_b.py \
  web/python/_manim_animate.py \
  web/python/_manim_reactive.py
PYTHONDONTWRITEBYTECODE=1 python3 -m compileall -q web/python/examples
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'
cargo test -p noon-web --test playground_examples

wasm-pack build crates/noon-web --target web --out-dir ../../web/pkg --release
node scripts/check-web-package.mjs
