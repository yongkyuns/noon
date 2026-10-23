#!/usr/bin/env bash
# Bound Ubuntu reference setup independently of expensive compilation/rendering.
set -euo pipefail

if (( $# == 0 )); then
  echo "Usage: bash scripts/ci-install-reference-deps.sh PACKAGE..." >&2
  exit 2
fi
for package in "$@"; do
  if [[ ! "$package" =~ ^[a-z0-9][a-z0-9.+:-]*$ ]]; then
    echo "Invalid reference dependency: $package" >&2
    exit 2
  fi
done

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
