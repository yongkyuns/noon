#!/usr/bin/env bash
# Bound Ubuntu reference setup independently of expensive compilation/rendering.
set -euo pipefail

if (( $# == 0 )); then
  echo "Usage: bash scripts/ci-install-reference-deps.sh PACKAGE... | --playwright BROWSER" >&2
  exit 2
fi
playwright_browser=
if [[ "$1" == --playwright ]]; then
  if (( $# != 2 )) || [[ ! "$2" =~ ^(chromium|firefox|webkit)$ ]]; then
    echo "Expected one supported Playwright browser" >&2
    exit 2
  fi
  playwright_browser=$2
  node_binary=$(node -p 'process.execPath')
  # Resolve the already-installed project package; never let npx fetch another
  # version or put browser downloads in root's cache.
  playwright_cli=$("$node_binary" -e '
    const fs = require("node:fs"), path = require("node:path");
    const packagePath = require.resolve("./node_modules/playwright/package.json");
    const manifest = require(packagePath);
    if (manifest.name !== "playwright" || manifest.bin?.playwright !== "cli.js")
      throw new Error("Unexpected installed Playwright CLI");
    const cli = path.join(path.dirname(packagePath), manifest.bin.playwright);
    fs.accessSync(cli, fs.constants.R_OK);
    process.stdout.write(cli);
  ')
else
  for package in "$@"; do
    if [[ ! "$package" =~ ^[a-z0-9][a-z0-9.+:-]*$ ]]; then
      echo "Invalid reference dependency: $package" >&2
      exit 2
    fi
  done
fi

# The hosted image's mirror list may keep a responsive but extremely slow Azure
# endpoint for the whole download. Use official HTTPS repositories for this
# invocation, retaining the image's suites, components and signing-key settings.
source_dir=$(mktemp -d)
trap 'rm -rf "$source_dir"' EXIT
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 -S "$script_dir/ci_reference_apt_sources.py" \
  /etc/apt/sources.list.d/ubuntu.sources > "$source_dir/ubuntu.sources"
chmod 755 "$source_dir"
chmod 644 "$source_dir/ubuntu.sources"

if [[ -n "$playwright_browser" ]]; then
  python3 -S "$script_dir/ci_reference_apt_sources.py" \
    --playwright-config "$source_dir/ubuntu.sources" > "$source_dir/apt.conf"
  chmod 600 "$source_dir/apt.conf"
  # Pass APT_CONFIG after sudo: Playwright's own sudo boundary would drop it.
  # Its update and install share the existing 180s + 600s setup budget. A system
  # PATH ensures its apt-get calls do not enter runner-image retry wrappers.
  sudo -n /usr/bin/timeout --signal=TERM --kill-after=15s 780s \
    env DEBIAN_FRONTEND=noninteractive APT_CONFIG="$source_dir/apt.conf" \
    PATH=/usr/sbin:/usr/bin:/sbin:/bin \
    "$node_binary" "$playwright_cli" install-deps "$playwright_browser"
  exit
fi

apt_options=(
  -o "Dir::Etc::sourcelist=$source_dir/ubuntu.sources"
  -o Dir::Etc::sourceparts=-
  -o Acquire::Retries=2
  -o Acquire::http::Timeout=20
  -o Acquire::https::Timeout=20
  -o DPkg::Lock::Timeout=60
)

# Use the system binary: runner-image apt wrappers may retry or mask failures.
# Run timeout with sudo too, so it can signal apt and its privileged children.
sudo -n /usr/bin/timeout --signal=TERM --kill-after=15s 180s \
  env DEBIAN_FRONTEND=noninteractive /usr/bin/apt-get \
  "${apt_options[@]}" --error-on=any update
sudo -n /usr/bin/timeout --signal=TERM --kill-after=15s 600s \
  env DEBIAN_FRONTEND=noninteractive /usr/bin/apt-get \
  "${apt_options[@]}" install -y --no-install-recommends "$@"
