#!/usr/bin/env bash
# Build tools inside cibuildwheel's pinned manylinux container. Runtime graphics
# drivers and FFmpeg are supplied by the consuming host, not bundled in a wheel.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
noon_rust_channel="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' "$ROOT/rust-toolchain.toml")"
test -n "$noon_rust_channel"
dnf install -y pkgconf-pkg-config libxkbcommon-devel
if ! command -v rustup >/dev/null 2>&1; then
  noon_rustup_installer="$(mktemp)"
  trap 'rm -f "$noon_rustup_installer"' EXIT
  curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o "$noon_rustup_installer"
  sh "$noon_rustup_installer" -y --profile minimal --default-toolchain "$noon_rust_channel"
fi
"$HOME/.cargo/bin/rustup" toolchain install "$noon_rust_channel" --profile minimal
