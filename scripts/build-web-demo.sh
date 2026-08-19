#!/usr/bin/env bash

set -euo pipefail

noon_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$noon_root"

wasm-pack build crates/noon-web --target web --out-dir ../../web/pkg --release
node --check web/main.js
node scripts/check-web-package.mjs
