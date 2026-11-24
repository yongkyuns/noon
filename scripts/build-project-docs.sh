#!/usr/bin/env bash
# Invoked by the Pages and PR staging CLI, never by the engine build.
set -euo pipefail
root="${1:?repository root required}"
site="${2:?staged site directory required}"
cd "$root"
python3 -m venv target/project-site-tools
python="${root}/target/project-site-tools/bin/python"
"$python" -m pip install --disable-pip-version-check -r scripts/site/requirements.txt
# A separate clean target prevents stale docs for removed APIs from being staged.
rm -rf target/project-site-rustdoc/doc
cargo doc --locked --no-deps --lib -p noon -p noon-native -p noon-export --target-dir target/project-site-rustdoc
revision="$(git rev-parse HEAD)"
"$python" - "$revision" <<'PY'
import json, sys
from pathlib import Path
Path('target/project-site-rustdoc/doc/noon-site-source.json').write_text(json.dumps({
    'revision': sys.argv[1], 'packages': ['noon', 'noon-native', 'noon-export'],
    'profile': 'default features, --no-deps, native target',
}) + '\n')
PY
"$python" scripts/site/build_docs.py --root "$root" --site "$site" --rustdoc target/project-site-rustdoc/doc
"$python" -m unittest discover -s scripts/site -p 'test_*.py' -v
