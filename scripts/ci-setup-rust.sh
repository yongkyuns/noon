#!/usr/bin/env bash
# Source this after checkout in GitHub Actions. Keep the repository toolchain pin,
# but avoid partially installed components in the hosted runner's Rustup home.
: "${RUNNER_TEMP:?CI Rust setup requires RUNNER_TEMP}"
: "${GITHUB_ENV:?CI Rust setup requires GITHUB_ENV}"
noon_rustup_home="$(mktemp -d "$RUNNER_TEMP/noon-rustup.XXXXXX")"
export RUSTUP_HOME="$noon_rustup_home"
printf 'RUSTUP_HOME=%s\n' "$RUSTUP_HOME" >> "$GITHUB_ENV"
rustup install
rustup show active-toolchain
rustc --version --verbose
