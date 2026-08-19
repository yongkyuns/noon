#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

node --check web/main.js
node --check web/authoring-client.js
node --check web/python-worker.js
node --test web/authoring-client.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s web/python -p 'test_*.py'

wasm-pack build crates/noon-web --target web --out-dir ../../web/pkg --release
node scripts/check-web-package.mjs
