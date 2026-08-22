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
node --test web/authoring-client.test.mjs
node --test web/scene-identity.test.mjs
node --test web/frame-metrics.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m compileall -q web/python/examples
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'
cargo test -p noon-web --test playground_examples

wasm-pack build crates/noon-web --target web --out-dir ../../web/pkg --release
node scripts/check-web-package.mjs
