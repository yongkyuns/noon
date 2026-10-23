#!/usr/bin/env bash
# Keep pinned browser downloads unprivileged; scope Ubuntu mirror selection to APT.
set -euo pipefail
if (( $# != 1 )) || [[ ! "$1" =~ ^(chromium|firefox|webkit)$ ]]; then
  echo "Usage: bash scripts/ci-install-playwright-browser.sh chromium|firefox|webkit" >&2
  exit 2
fi
browser="$1"
node_binary=$(command -v node)
playwright_package=$("$node_binary" -p "require.resolve('playwright/package.json')")
playwright_cli="${playwright_package%/*}/cli.js"
if [[ ! -f "$playwright_cli" ]]; then
  echo "Installed Playwright CLI was not found: $playwright_cli" >&2
  exit 1
fi
if [[ "$(uname -s)" != Linux ]]; then
  exec "$node_binary" "$playwright_cli" install --with-deps "$browser"
fi

# Reuse the same URI-only transformation as the pinned Manim reference setup.
# Neither the runner's source files nor unrelated repositories are modified.
source_dir=$(mktemp -d)
trap 'rm -rf "$source_dir"' EXIT
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -S "$script_dir/ci_reference_apt_sources.py"   /etc/apt/sources.list.d/ubuntu.sources > "$source_dir/ubuntu.sources"
printf 'Dir::Etc::sourcelist "%s";\nDir::Etc::sourceparts "-";\n'   "$source_dir/ubuntu.sources" > "$source_dir/apt.conf"
chmod 755 "$source_dir"
chmod 644 "$source_dir/ubuntu.sources" "$source_dir/apt.conf"

# Playwright invokes sudo itself when unprivileged, which would discard APT_CONFIG.
# Start only its dependency command as root, with the invocation-local config.
# Its own pinned package selection and update/install failure behavior are retained.
sudo -n env "APT_CONFIG=$source_dir/apt.conf"   "$node_binary" "$playwright_cli" install-deps "$browser"
"$node_binary" "$playwright_cli" install "$browser"
