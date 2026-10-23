#!/usr/bin/env bash
# Keep pinned browser downloads unprivileged; scope Ubuntu mirror selection to APT.
set -euo pipefail
if (( $# != 1 )) || [[ ! "$1" =~ ^(chromium|firefox|webkit)$ ]]; then
  echo "Usage: bash scripts/ci-install-playwright-browser.sh chromium|firefox|webkit" >&2
  exit 2
fi
browser="$1"
node_binary=$(node -p 'process.execPath')
playwright_package=$("$node_binary" -p "require.resolve('playwright/package.json')")
playwright_cli="${playwright_package%/*}/cli.js"
if [[ ! -f "$playwright_cli" ]]; then
  echo "Installed Playwright CLI was not found: $playwright_cli" >&2
  exit 1
fi
if [[ "$(uname -s)" != Linux ]]; then
  exec "$node_binary" "$playwright_cli" install --with-deps "$browser"
fi

# Share the bounded dependency installation and invocation-local APT settings
# with the reference workflows. The helper elevates only the dependency command;
# browser downloads below use the invoking user's cache.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$script_dir/ci-install-reference-deps.sh" --playwright "$browser"
"$node_binary" "$playwright_cli" install "$browser"
