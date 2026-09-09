#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/noon-core-module-ownership-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/crates/noon-core/src/reactive" \
  "$TMP/crates/noon-core/src/resources"
cp "$RATCHET" "$TMP/scripts/noon-core-module-ownership-ratchet.sh"
cd "$TMP"
git init -q
git config user.name "Noon Core Module Ownership Ratchet Test"
git config user.email "ratchet-test@example.invalid"

# Ordinary Rust ownership: root declarations resolve beside lib.rs; reactive
# children resolve below reactive/. Exercise both supported module file layouts.
for owner in animation publication reactive resources semantic_store; do
  printf 'mod %s;\n' "$owner" >> crates/noon-core/src/lib.rs
done
printf 'pub struct SemanticStore;\n' > crates/noon-core/src/semantic_store.rs
printf 'pub struct Animation;\n' > crates/noon-core/src/animation.rs
printf 'pub struct Revision;\n' > crates/noon-core/src/publication.rs
printf 'pub struct Resource;\n' > crates/noon-core/src/resources/mod.rs
printf 'mod ordinary;\n' > crates/noon-core/src/reactive.rs
printf 'pub fn ordinary() {}\n' > crates/noon-core/src/reactive/ordinary.rs
cp -R crates/noon-core/src "$TMP/baseline"
git add scripts crates/noon-core/src
git commit -qm "normalized noon-core baseline"

reset_sources() {
  rm -rf crates/noon-core/src
  cp -R "$TMP/baseline" crates/noon-core/src
}

expect_rejected() {
  local label="$1" diagnostic="$2" output
  if output="$(bash scripts/noon-core-module-ownership-ratchet.sh 2>&1)"; then
    echo "noon-core module ownership ratchet test failed: accepted $label" >&2
    exit 1
  fi
  if [[ "$output" != *"$diagnostic"* ]]; then
    printf 'noon-core module ownership ratchet test failed: wrong rejection for %s:\n%s\n' \
      "$label" "$output" >&2
    exit 1
  fi
}

bash scripts/noon-core-module-ownership-ratchet.sh >/dev/null

# This valid ordinary-path relocation passed the old indirection-only guard.
# Crate-root reexports can preserve type access while hiding ownership again.
for owner in animation publication resources semantic_store; do
  sed "/^mod $owner;$/d" crates/noon-core/src/lib.rs > "$TMP/lib.rs"
  mv "$TMP/lib.rs" crates/noon-core/src/lib.rs
  printf 'pub mod %s;\n' "$owner" >> crates/noon-core/src/reactive.rs
  if [[ -f "crates/noon-core/src/$owner.rs" ]]; then
    mv "crates/noon-core/src/$owner.rs" "crates/noon-core/src/reactive/$owner.rs"
  else
    mv "crates/noon-core/src/$owner" "crates/noon-core/src/reactive/$owner"
  fi
  expect_rejected "$owner relocated under reactive" "$owner must remain an ordinary private root module"
  reset_sources

  # Retaining the root declaration must not admit a parallel reactive owner.
  printf 'pub(crate) mod %s {}\n' "$owner" >> crates/noon-core/src/reactive/ordinary.rs
  expect_rejected "duplicate reactive $owner owner" 'unrelated domain declared under reactive'
  reset_sources
done

for owner in animation publication reactive resources semantic_store; do
  sed "s/^mod $owner;$/pub mod $owner;/" crates/noon-core/src/lib.rs > "$TMP/lib.rs"
  mv "$TMP/lib.rs" crates/noon-core/src/lib.rs
  expect_rejected "public $owner implementation module" "$owner must remain an ordinary private root module"
  reset_sources
done

# Equivalent ordinary mod.rs placement is allowed, not frozen to file.rs.
mkdir -p crates/noon-core/src/semantic_store
mv crates/noon-core/src/semantic_store.rs crates/noon-core/src/semantic_store/mod.rs
mv crates/noon-core/src/reactive.rs crates/noon-core/src/reactive/mod.rs
bash scripts/noon-core-module-ownership-ratchet.sh >/dev/null
reset_sources

rm crates/noon-core/src/semantic_store.rs
expect_rejected 'missing ordinary module file' 'semantic_store must resolve to exactly one ordinary module file'
reset_sources
mkdir -p crates/noon-core/src/semantic_store
cp crates/noon-core/src/semantic_store.rs crates/noon-core/src/semantic_store/mod.rs
expect_rejected 'ambiguous ordinary module files' 'semantic_store must resolve to exactly one ordinary module file'
reset_sources
rm crates/noon-core/src/lib.rs
expect_rejected 'missing root declaration file' 'root ownership scan failed'
reset_sources

# The retired reactive-to-semantic-store seam must not return.
printf '#[path = "semantic_store.rs"]\nmod semantic_store;\n' > crates/noon-core/src/reactive.rs
expect_rejected 'retired semantic-store path indirection' 'unexpected indirection'
reset_sources

# A failed scanner must not be interpreted as an empty/valid source tree.
mkdir -p "$TMP/failing-tools"
printf '#!/usr/bin/env bash\nexit 2\n' > "$TMP/failing-tools/grep"
chmod +x "$TMP/failing-tools/grep"
PATH="$TMP/failing-tools:$PATH" expect_rejected 'source scanner failure' 'source scan failed'

# Also fail closed when only the new reactive-domain scan fails.
real_grep="$(command -v grep)"
printf '#!/usr/bin/env bash\nif [[ "${!#}" == */reactive ]]; then exit 2; fi\nexec %q "$@"\n' \
  "$real_grep" > "$TMP/failing-tools/grep"
PATH="$TMP/failing-tools:$PATH" expect_rejected 'reactive scanner failure' 'reactive ownership scan failed'

# Untracked files must still be scanned, as in the original guard.
printf '#[path = "hidden_impl.rs"]\nmod hidden_impl;\n' > crates/noon-core/src/hidden.rs
printf 'pub fn hidden_impl() {}\n' > crates/noon-core/src/hidden_impl.rs
expect_rejected 'additional #[path] indirection' 'unexpected indirection'
reset_sources

for delimiters in '("hidden_impl.rs")' '{"hidden_impl.rs"}' '["hidden_impl.rs"]'; do
  printf 'include!%s;\n' "$delimiters" > crates/noon-core/src/hidden.rs
  printf 'pub fn hidden_impl() {}\n' > crates/noon-core/src/hidden_impl.rs
  expect_rejected 'additional include! indirection' 'unexpected indirection'
  reset_sources
done

bash scripts/noon-core-module-ownership-ratchet.sh >/dev/null
echo "noon-core module ownership ratchet self-test passed"
