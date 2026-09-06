#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/architecture-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/src" "$TMP/web" "$TMP/crates/noon-core/src" "$TMP/crates/noon-runtime/src" "$TMP/crates/noon-web/src"
cp "$RATCHET" "$TMP/scripts/architecture-ratchet.sh"
cp "$ROOT/scripts/architecture_migration_relocations.py" "$ROOT/scripts/architecture_migration_relocations.json" "$TMP/scripts/"
REVIEWED_RELOCATION_CONFIG="$(cat "$TMP/scripts/architecture_migration_relocations.json")"
python3 - <<'PY' "$TMP/scripts/architecture_migration_relocations.json"
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
config = json.loads(path.read_text())
config.pop('regression_fixtures', None)
path.write_text(json.dumps(config, indent=2) + '\n')
PY

cd "$TMP"
git init -q
git config user.name "Noon Architecture Ratchet Test"
git config user.email "ratchet-test@example.invalid"

# Model the canonical semantic identity authority established by Phase A1.
cat > crates/noon-core/src/semantic_store.rs <<'EOF'
pub struct SemanticNodeId {
    slot: u32,
    generation: u32,
}

impl SemanticNodeId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }
}

pub struct SemanticStore;
EOF

# Model the repository during Phase A: old module indirection may still exist
# outside ownership islands already normalized by A5. The growth ratchet must
# prevent that debt from spreading while cleaned islands stay structurally clean.
cat > src/lib.rs <<'EOF'
#[path = "existing_hidden.rs"]
mod existing_hidden;
include!("existing_impl.rs");
EOF
printf 'pub fn existing_hidden() {}\n' > src/existing_hidden.rs
printf 'pub fn existing_impl() {}\n' > src/existing_impl.rs

# noon-runtime models a post-A5 normalized island: only ordinary module layout.
cat > crates/noon-runtime/src/lib.rs <<'EOF'
mod runtime;
EOF
printf 'pub fn runtime() {}\n' > crates/noon-runtime/src/runtime.rs

# These web tools were detached from the migration player by #991 and #994.
printf 'pub fn deterministic_replay() {}\n' > crates/noon-web/src/determinism.rs
printf 'pub fn semantic_snapshot() {}\n' > crates/noon-web/src/semantic_snapshot.rs

# Model the deliberately shrinking ScenePlayer allowlist that remains during A4.
cat > crates/noon-web/src/legacy.rs <<'EOF'
pub struct ScenePlayer;
EOF
for consumer in execution_transport; do
  cat > "crates/noon-web/src/${consumer}.rs" <<'EOF'
use crate::ScenePlayer;
EOF
done

# Model the canonical playback clock left after #1005 removed its legacy duplicate.
printf 'pub struct PlaybackClock;\n' > crates/noon-web/src/clock.rs

git add scripts src crates/noon-core/src/semantic_store.rs crates/noon-runtime/src crates/noon-web/src web
git commit -qm "baseline with semantic authority, known A5 debt, and normalized islands"
BASE="$(git rev-parse HEAD)"

printf 'pub fn visible_module_tree() {}\n' > src/visible.rs
git add src/visible.rs
git commit -qm "unrelated clean change"
bash scripts/architecture-ratchet.sh "$BASE" >/dev/null

expect_rejected() {
  label="$1"
  if bash scripts/architecture-ratchet.sh "$BASE" >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted $label" >&2
    exit 1
  fi
}

reset_to_base() {
  git reset -q --hard "$BASE"
  rm -f src/new_hidden.rs src/duplicate_identity.rs src/runtime_structural_probe.rs src/web_tool_structural_probe.rs src/scene_player_spread_probe.rs src/deleted_legacy_web_probe.rs src/duplicate_clock_probe.rs src/legacy_clock_probe.rs
  rm -f web/browser-smoke.js crates/noon-web/src/duplicate_clock.rs crates/noon-web/src/legacy/clock.rs
  rm -rf crates/noon-web/src/retained_execution_resources crates/noon-web/src/retained_resource_transport
  rm -f crates/noon-web/src/retained_execution_resources.rs crates/noon-web/src/retained_resource_transport.rs scripts/retained-dynamic-stress-perf.mjs
  rmdir crates/noon-web/src/legacy 2>/dev/null || true
}

# The one deletion-owned payload fixture is permitted only under cfg(test).
reset_to_base
mkdir -p crates/noon-compile/src/transaction_preflight
cat > crates/noon-compile/src/transaction_preflight/tests.rs <<'EOF'
#![cfg(test)]
use noon_core::ObjectDefinition;
EOF
git add crates/noon-compile
git commit -qm "test-only patch payload fixture"
bash scripts/architecture-ratchet.sh "$BASE" >/dev/null

printf 'use noon_core::SceneDefinition;\n' >> crates/noon-compile/src/transaction_preflight/tests.rs
git add crates/noon-compile
git commit -qm "forbidden scene builder in fixture"
expect_rejected 'scene builder in payload fixture'

reset_to_base
mkdir -p crates/noon-compile/src/transaction_preflight
cat > crates/noon-compile/src/transaction_preflight/tests.rs <<'EOF'
use noon_core::ObjectDefinition;
EOF
git add crates/noon-compile
git commit -qm "patch payload without test-only gate"
expect_rejected 'payload fixture without cfg(test)'

reset_to_base
printf 'use noon_core::ObjectDefinition;\n' > src/payload.rs
git add src/payload.rs
git commit -qm "production patch payload growth"
expect_rejected 'production patch payload growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
#[path = "another_hidden.rs"]
mod another_hidden;
EOF
git add src/new_hidden.rs
git commit -qm "add path indirection"
expect_rejected '#[path] module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include!("another_hidden.rs");
EOF
git add src/new_hidden.rs
git commit -qm "add include parens"
expect_rejected 'include!(...) module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include! { "another_hidden.rs" }
EOF
git add src/new_hidden.rs
git commit -qm "add include braces"
expect_rejected 'include! {...} module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include! [ "another_hidden.rs" ]
EOF
git add src/new_hidden.rs
git commit -qm "add include brackets"
expect_rejected 'include! [...] module indirection growth'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
pub struct SemanticNodeId(u64);
EOF
git add src/duplicate_identity.rs
git commit -qm "duplicate semantic node identity"
expect_rejected 'second SemanticNodeId definition'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
pub struct SemanticStore;
EOF
git add src/duplicate_identity.rs
git commit -qm "duplicate semantic store"
expect_rejected 'second SemanticStore definition'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
impl SemanticNodeId {
    pub fn fabricated() -> Self { unreachable!() }
}
EOF
git add src/duplicate_identity.rs
git commit -qm "move semantic id allocator api"
expect_rejected 'SemanticNodeId inherent implementation outside canonical owner'

# Prove noon-runtime is no longer merely growth-ratcheted. Put hidden ownership
# into the comparison base itself, then make an unrelated later commit. A diff-
# only growth check cannot see the old line; the full-tree normalized-island gate
# still must reject the repository state.
reset_to_base
cat > crates/noon-runtime/src/lib.rs <<'EOF'
#[path = "runtime_hidden.rs"]
mod runtime_hidden;
EOF
printf 'pub fn runtime_hidden() {}\n' > crates/noon-runtime/src/runtime_hidden.rs
git add crates/noon-runtime/src
git commit -qm "model regressed normalized runtime baseline"
RUNTIME_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_runtime_regression() {}\n' > src/runtime_structural_probe.rs
git add src/runtime_structural_probe.rs
git commit -qm "unrelated change after runtime regression"
if bash scripts/architecture-ratchet.sh "$RUNTIME_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted pre-existing noon-runtime module indirection" >&2
  exit 1
fi

# Prove the completed A4.6 tool cutovers are structural, not just growth-ratcheted.
# Put migration-player dependencies into the comparison base itself, then make an
# unrelated later commit. The full-tree guard must still reject both tool paths.
reset_to_base
cat > crates/noon-web/src/determinism.rs <<'EOF'
use crate::ScenePlayer;
pub fn deterministic_replay() {}
EOF
cat > crates/noon-web/src/semantic_snapshot.rs <<'EOF'
use crate::PlayerError;
pub fn semantic_snapshot() {}
EOF
git add crates/noon-web/src
git commit -qm "model regressed web tool player baseline"
WEB_TOOL_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_web_tool_regression() {}\n' > src/web_tool_structural_probe.rs
git add src/web_tool_structural_probe.rs
git commit -qm "unrelated change after web tool regression"
if bash scripts/architecture-ratchet.sh "$WEB_TOOL_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted pre-existing web tool migration-player dependency" >&2
  exit 1
fi

# Prove ScenePlayer cannot spread to another noon-web Rust module. Put a new
# consumer into the comparison base itself, then make an unrelated later commit;
# the structural allowlist must still reject that repository state.
reset_to_base
cat > crates/noon-web/src/new_scene_player_consumer.rs <<'EOF'
use crate::ScenePlayer;
EOF
git add crates/noon-web/src/new_scene_player_consumer.rs
git commit -qm "model ScenePlayer consumer spread"
SCENE_PLAYER_SPREAD_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_scene_player_spread() {}\n' > src/scene_player_spread_probe.rs
git add src/scene_player_spread_probe.rs
git commit -qm "unrelated change after ScenePlayer spread"
if bash scripts/architecture-ratchet.sh "$SCENE_PLAYER_SPREAD_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted ScenePlayer consumer outside migration allowlist" >&2
  exit 1
fi

# Prove the cleaned #1003 primary browser smoke cannot regain the deleted frontend
# even when the regression predates the current diff.
reset_to_base
cat > web/browser-smoke.js <<'EOF'
export class NoonCanvasPlayer {}
export function demoSceneJson() { return "{}"; }
EOF
git add web/browser-smoke.js
git commit -qm "model deleted primary browser frontend returning"
DELETED_FRONTEND_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_deleted_frontend() {}\n' > src/deleted_legacy_web_probe.rs
git add src/deleted_legacy_web_probe.rs
git commit -qm "unrelated change after deleted frontend regression"
if bash scripts/architecture-ratchet.sh "$DELETED_FRONTEND_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted deleted primary NoonCanvasPlayer/demoSceneJson surface" >&2
  exit 1
fi

# Prove playback clock ownership remains singular after #1005.
reset_to_base
printf 'pub struct PlaybackClock;\n' > crates/noon-web/src/duplicate_clock.rs
git add crates/noon-web/src/duplicate_clock.rs
git commit -qm "model duplicate playback clock"
DUPLICATE_CLOCK_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_duplicate_clock() {}\n' > src/duplicate_clock_probe.rs
git add src/duplicate_clock_probe.rs
git commit -qm "unrelated change after duplicate clock regression"
if bash scripts/architecture-ratchet.sh "$DUPLICATE_CLOCK_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted duplicate PlaybackClock authority" >&2
  exit 1
fi

# Prove the exact deleted legacy clock module path cannot be recreated even if it
# does not itself declare another PlaybackClock yet.
reset_to_base
mkdir -p crates/noon-web/src/legacy
printf 'pub fn stale_clock_module() {}\n' > crates/noon-web/src/legacy/clock.rs
git add crates/noon-web/src/legacy/clock.rs
git commit -qm "model deleted legacy clock path returning"
LEGACY_CLOCK_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_legacy_clock() {}\n' > src/legacy_clock_probe.rs
git add src/legacy_clock_probe.rs
git commit -qm "unrelated change after legacy clock regression"
if bash scripts/architecture-ratchet.sh "$LEGACY_CLOCK_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted deleted legacy clock module path" >&2
  exit 1
fi

# #959 namespace relocation is explicit and symbol-preserving, including grouped
# imports. It grants no new file, alias, glob, or non-import namespace access.
reset_to_base
mkdir -p crates/noon-web/src crates/noon/src/legacy
cat > crates/noon-web/src/reactive_authoring_facade.rs <<'EOF'
use noon::{Circle, Mobject, ReactiveTimelineScene};
EOF
git add crates/noon-web/src/reactive_authoring_facade.rs
git commit -qm "existing unqualified import consumer"
IMPORT_BASE="$(git rev-parse HEAD)"
cat > crates/noon-web/src/reactive_authoring_facade.rs <<'EOF'
use noon::legacy::{
    Circle,
    Mobject,
};
use noon::ReactiveTimelineScene;
EOF
bash scripts/architecture-ratchet.sh "$IMPORT_BASE" >/dev/null
for import in 'use noon::legacy::{Circle as Hidden, Mobject};' 'use noon::legacy::*;' 'use noon::legacy::{Circle, Unknown};' 'use noon::{legacy::{Circle as Hidden, Mobject}};' 'use noon::legacy;' 'use noon::{legacy};' 'use noon::legacy as old;' 'use noon::r#legacy::Circle;' 'use noon::legacy::Circle @ unsupported;'; do
  printf '%s\n' "$import" > crates/noon-web/src/reactive_authoring_facade.rs
  if bash scripts/architecture-ratchet.sh "$IMPORT_BASE" >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted unreviewed import $import" >&2
    exit 1
  fi
done
git reset -q --hard "$IMPORT_BASE"
printf 'use noon::legacy::Circle;\n' > crates/noon-web/src/new_legacy_consumer.rs
if bash scripts/architecture-ratchet.sh "$IMPORT_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted an untracked new legacy consumer" >&2
  exit 1
fi
rm crates/noon-web/src/new_legacy_consumer.rs

# The initial codec allowance requires deleting the old snapshot authority.
reset_to_base
mkdir -p crates/noon/src/legacy
cat > crates/noon-web/src/authoring_mobject.rs <<'EOF'
pub struct FrontendMobjectHandle { snapshot: ObjectSnapshot }
EOF
git add crates/noon-web/src/authoring_mobject.rs
git commit -qm "old snapshot handle before relocation"
RELOCATION_BASE="$(git rev-parse HEAD)"
cat > crates/noon/src/legacy/semantic_snapshot.rs <<'EOF'
use noon_core::ObjectSnapshot;
pub fn export_mobject_snapshot() -> ObjectSnapshot { todo!() }
EOF
if bash scripts/architecture-ratchet.sh "$RELOCATION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted relocation retaining old authority" >&2
  exit 1
fi
printf 'pub struct WasmAuthoringMobjectHandle { handle: noon::Mobject }\n' > crates/noon-web/src/authoring_mobject.rs
bash scripts/architecture-ratchet.sh "$RELOCATION_BASE" >/dev/null
printf 'impl noon::Mobject { pub fn snapshot() {} }\n' >> crates/noon/src/legacy/semantic_snapshot.rs
if bash scripts/architecture-ratchet.sh "$RELOCATION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted inherent snapshot API in codec" >&2
  exit 1
fi
sed -i.bak '$d' crates/noon/src/legacy/semantic_snapshot.rs
rm crates/noon/src/legacy/semantic_snapshot.rs.bak
# Direct calls must obey the same API/count inventory despite whitespace or
# raw-identifier spellings of a Rust namespace.
for call in 'noon :: legacy :: NewApi();' 'noon::r#legacy::NewApi();'; do
  printf 'pub fn probe() { %s }\n' "$call" >> crates/noon-web/src/authoring_mobject.rs
  if bash scripts/architecture-ratchet.sh "$RELOCATION_BASE" >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted unreviewed spaced/raw adapter call" >&2
    exit 1
  fi
  sed -i.bak '$d' crates/noon-web/src/authoring_mobject.rs
  rm crates/noon-web/src/authoring_mobject.rs.bak
done
git add crates/noon/src/legacy/semantic_snapshot.rs crates/noon-web/src/authoring_mobject.rs
git commit -qm "bounded codec relocation"
MOVED_BASE="$(git rev-parse HEAD)"
printf 'pub fn probe() { noon :: legacy :: export_mobject_snapshot(); }\n' >> crates/noon-web/src/authoring_mobject.rs
if bash scripts/architecture-ratchet.sh "$MOVED_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: spaced approved call bypassed zero baseline budget" >&2
  exit 1
fi
sed -i.bak '$d' crates/noon-web/src/authoring_mobject.rs
rm crates/noon-web/src/authoring_mobject.rs.bak
printf 'pub fn export_mobject_snapshot() {}\n' > crates/noon/src/legacy/semantic_snapshot.rs
git add crates/noon/src/legacy/semantic_snapshot.rs
git commit -qm "shrink codec debt"
SHRUNK_BASE="$(git rev-parse HEAD)"
printf 'use noon_core::ObjectSnapshot;\n' >> crates/noon/src/legacy/semantic_snapshot.rs
if bash scripts/architecture-ratchet.sh "$SHRUNK_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted regrowth under initial codec cap" >&2
  exit 1
fi

# #959 regression fixtures may carry exact migration payloads only in reviewed
# files. Rust fixtures must be crate-test-only and attached through an ordinary,
# cfg-gated module. The first reviewed config entry establishes its cap; every
# later comparison ratchets against the base count, including deletion.
reset_to_base
printf '%s\n' "$REVIEWED_RELOCATION_CONFIG" > scripts/architecture_migration_relocations.json
mkdir -p crates/noon-web/src/retained_execution_resources crates/noon-web/src/retained_resource_transport
cat > crates/noon-web/src/retained_execution_resources.rs <<'EOF'
#[cfg(test)]
mod morph_tests;
EOF
cat > crates/noon-web/src/retained_execution_resources/morph_tests.rs <<'EOF'
#![cfg(test)]
// SceneDefinition SceneDefinition ObjectSnapshot ObjectSnapshot ObjectSnapshot
EOF
cat > crates/noon-web/src/retained_resource_transport.rs <<'EOF'
#[cfg(test)]
mod morph_tests;
EOF
cat > crates/noon-web/src/retained_resource_transport/morph_tests.rs <<'EOF'
#![cfg(test)]
// ObjectSnapshot ObjectSnapshot ObjectSnapshot
// RetainedObjectDefinition ObjectDefinition
EOF
cat > scripts/retained-dynamic-stress-perf.mjs <<'EOF'
// Explicit export regression fixture: scene_document SceneSpec
EOF
bash scripts/architecture-ratchet.sh "$BASE" >/dev/null

sed -i.bak '1s/.*/\/\/ missing crate test gate/' crates/noon-web/src/retained_execution_resources/morph_tests.rs
expect_rejected 'regression fixture without first-line cfg(test)'
rm crates/noon-web/src/retained_execution_resources/morph_tests.rs.bak
sed -i.bak '1s/.*/#![cfg(test)]/' crates/noon-web/src/retained_execution_resources/morph_tests.rs
rm crates/noon-web/src/retained_execution_resources/morph_tests.rs.bak

printf 'mod morph_tests;\n' > crates/noon-web/src/retained_execution_resources.rs
expect_rejected 'regression fixture through an ungated parent module'
cat > crates/noon-web/src/retained_execution_resources.rs <<'EOF'
#[cfg(test)]
mod morph_tests;
EOF
printf '// ObjectSnapshot moved into production parent\n' >> crates/noon-web/src/retained_execution_resources.rs
expect_rejected 'fixture token moved into its production parent'
sed -i.bak '$d' crates/noon-web/src/retained_execution_resources.rs
rm crates/noon-web/src/retained_execution_resources.rs.bak

printf '// SceneSpec is not in this fixture inventory\n' >> crates/noon-web/src/retained_execution_resources/morph_tests.rs
expect_rejected 'new unreviewed fixture token'
sed -i.bak '$d' crates/noon-web/src/retained_execution_resources/morph_tests.rs
rm crates/noon-web/src/retained_execution_resources/morph_tests.rs.bak
printf '// ObjectSnapshot exceeds the reviewed cap\n' >> crates/noon-web/src/retained_execution_resources/morph_tests.rs
expect_rejected 'regression fixture cap growth'
sed -i.bak '$d' crates/noon-web/src/retained_execution_resources/morph_tests.rs
rm crates/noon-web/src/retained_execution_resources/morph_tests.rs.bak

git add scripts/architecture_migration_relocations.json scripts/retained-dynamic-stress-perf.mjs crates/noon-web/src/retained_execution_resources.rs crates/noon-web/src/retained_execution_resources/morph_tests.rs crates/noon-web/src/retained_resource_transport.rs crates/noon-web/src/retained_resource_transport/morph_tests.rs
git commit -qm 'review bounded migration regression fixtures'
FIXTURE_BASE="$(git rev-parse HEAD)"
python3 - <<'PY'
import json
import pathlib

path = pathlib.Path('scripts/architecture_migration_relocations.json')
config = json.loads(path.read_text())
del config['regression_fixtures']['crates/noon-web/src/retained_execution_resources/morph_tests.rs']
path.write_text(json.dumps(config, indent=2) + '\n')
PY
if bash scripts/architecture-ratchet.sh "$FIXTURE_BASE" >/dev/null 2>&1; then
  echo 'architecture ratchet test failed: accepted fixture budget entry deletion' >&2
  exit 1
fi
git checkout -q -- scripts/architecture_migration_relocations.json
sed -i.bak 's/ObjectSnapshot ObjectSnapshot ObjectSnapshot/ObjectSnapshot ObjectSnapshot/' crates/noon-web/src/retained_execution_resources/morph_tests.rs
rm crates/noon-web/src/retained_execution_resources/morph_tests.rs.bak
git add crates/noon-web/src/retained_execution_resources/morph_tests.rs
git commit -qm 'shrink regression fixture payload'
FIXTURE_SHRUNK_BASE="$(git rev-parse HEAD)"
printf '// ObjectSnapshot cannot regrow after shrink\n' >> crates/noon-web/src/retained_execution_resources/morph_tests.rs
if bash scripts/architecture-ratchet.sh "$FIXTURE_SHRUNK_BASE" >/dev/null 2>&1; then
  echo 'architecture ratchet test failed: accepted fixture regrowth after shrink' >&2
  exit 1
fi
git reset -q --hard "$FIXTURE_SHRUNK_BASE"
git rm -q crates/noon-web/src/retained_execution_resources/morph_tests.rs
printf '' > crates/noon-web/src/retained_execution_resources.rs
git add crates/noon-web/src/retained_execution_resources.rs
git commit -qm 'delete regression fixture while retaining tombstone budget'
FIXTURE_DELETED_BASE="$(git rev-parse HEAD)"
mkdir -p crates/noon-web/src/retained_execution_resources
cat > crates/noon-web/src/retained_execution_resources.rs <<'EOF'
#[cfg(test)]
mod morph_tests;
EOF
cat > crates/noon-web/src/retained_execution_resources/morph_tests.rs <<'EOF'
#![cfg(test)]
// ObjectSnapshot
EOF
if bash scripts/architecture-ratchet.sh "$FIXTURE_DELETED_BASE" >/dev/null 2>&1; then
  echo 'architecture ratchet test failed: accepted deleted fixture re-addition' >&2
  exit 1
fi

# Canonical authoring remains an absolute-zero island, even for tests and even
# when the regression was committed before the comparison base.
reset_to_base
mkdir -p crates/noon/src/semantic_mobject crates/noon/src/scene
for canonical in crates/noon/src/semantic_mobject.rs crates/noon/src/scene.rs crates/noon/src/semantic_mobject/tests.rs crates/noon/src/scene/tests.rs; do
  printf 'use noon_core::ObjectSnapshot;\n' > "$canonical"
  git add "$canonical"
  git commit -qm "poison canonical authoring island"
  POISONED_BASE="$(git rev-parse HEAD)"
  if bash scripts/architecture-ratchet.sh "$POISONED_BASE" >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted canonical snapshot state in $canonical" >&2
    exit 1
  fi
  git rm -q "$canonical"
  git commit -qm "remove canonical poison"
done
# Lowering and live publication must stay typed even when an old patch-codec
# dependency already exists at the comparison base.
for canonical in crates/noon-compile/src/semantic_lowering.rs crates/noon-compile/src/semantic_lowering/publication.rs crates/noon/src/execution_session.rs crates/noon/src/execution_session/publication.rs crates/noon/src/live_session.rs; do
  mkdir -p "$(dirname "$canonical")"
  printf 'use noon_core::{ScenePatch, MutationTransaction};\n' > "$canonical"
  git add "$canonical"
  git commit -qm 'poison canonical execution boundary'
  if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted external patch codec in $canonical" >&2
    exit 1
  fi
  git rm -q "$canonical"
  git commit -qm 'remove execution boundary poison'
done
mkdir -p crates/noon/src
for export in 'pub use legacy::*;' 'pub use crate::legacy::{Scene as Mobject};' 'pub use legacy::{*};' 'pub use crate::legacy as old;' 'pub use crate::{legacy};' 'pub use self::legacy::*;' 'pub type Scene = crate::legacy::Scene;' 'use crate::legacy as old; pub type Mobject = old::Mobject;'; do
  printf '%s\n' "$export" > crates/noon/src/lib.rs
  if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted legacy root reexport $export" >&2
    exit 1
  fi
done
rm crates/noon/src/lib.rs

# Namespace-root aliases must not bypass the canonical full-tree check.
mkdir -p crates/noon/src
printf 'pub fn probe() { super :: super :: legacy :: call(); }\n' > crates/noon/src/scene.rs
if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted canonical legacy namespace alias" >&2
  exit 1
fi
rm crates/noon/src/scene.rs
printf 'type Restored = FrontendMobjectHandle;\n' > src/restored_handle.rs
if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted old handle through a type alias" >&2
  exit 1
fi
rm src/restored_handle.rs

# Retired reactive runtime symbols are forbidden even through a new alias/consumer.
for retired in TimedScenePlayer ReactiveScenePlayer ReactiveCanvasPlayer WasmReactiveScenePlayer WasmReactiveCanvasPlayer NativeInputRouter; do
  printf 'type Restored = %s;\n' "$retired" > src/restored_reactive.rs
  if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted retired reactive symbol $retired" >&2
    exit 1
  fi
  rm src/restored_reactive.rs
done

# Tool vocabulary is exempt at exactly its enforcement path, never when copied
# into product code. Missing comparison commits also fail closed.
reset_to_base
cp scripts/architecture_migration_relocations.py src/copied_checker.py
expect_rejected 'checker vocabulary copied into product code'
rm src/copied_checker.py
if bash scripts/architecture-ratchet.sh unavailable-base-959 >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted missing comparison base" >&2
  exit 1
fi

echo "architecture ratchet self-test passed"
