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

apt_options=(
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
