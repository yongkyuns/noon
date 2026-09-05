#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/noon-core-module-ownership-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/crates/noon-core/src"
cp "$RATCHET" "$TMP/scripts/noon-core-module-ownership-ratchet.sh"
cd "$TMP"
git init -q
git config user.name "Noon Core Module Ownership Ratchet Test"
git config user.email "ratchet-test@example.invalid"

printf 'mod semantic_store;\nmod ordinary;\n' > crates/noon-core/src/reactive.rs
printf 'pub struct SemanticStore;\n' > crates/noon-core/src/semantic_store.rs
printf 'pub fn ordinary() {}\n' > crates/noon-core/src/ordinary.rs
git add scripts crates/noon-core/src
git commit -qm "normalized noon-core baseline"

# Fully ordinary ownership is valid: the ratchet must not preserve the
# temporary seam forever.
bash scripts/noon-core-module-ownership-ratchet.sh >/dev/null

expect_rejected() {
  label="$1"
  if bash scripts/noon-core-module-ownership-ratchet.sh >/dev/null 2>&1; then
    echo "noon-core module ownership ratchet test failed: accepted $label" >&2
    exit 1
  fi
}

# A failed scanner must not be interpreted as an empty/valid source tree.
mkdir -p "$TMP/failing-tools"
printf '#!/usr/bin/env bash\nexit 2\n' > "$TMP/failing-tools/grep"
chmod +x "$TMP/failing-tools/grep"
PATH="$TMP/failing-tools:$PATH" expect_rejected 'source scanner failure'

printf '#[path = "hidden_impl.rs"]\nmod hidden_impl;\n' > crates/noon-core/src/hidden.rs
printf 'pub fn hidden_impl() {}\n' > crates/noon-core/src/hidden_impl.rs
expect_rejected 'additional #[path] indirection'
rm crates/noon-core/src/hidden.rs crates/noon-core/src/hidden_impl.rs

printf 'include!("hidden_impl.rs");\n' > crates/noon-core/src/hidden.rs
printf 'pub fn hidden_impl() {}\n' > crates/noon-core/src/hidden_impl.rs
expect_rejected 'additional include! indirection'

echo "noon-core module ownership ratchet self-test passed"
