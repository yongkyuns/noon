# Phase A serialization consumer inventory

Status: Phase A / A4.1 inventory for #959.

This document is an implementation ledger, not a second architecture document. `docs/architecture.md` remains authoritative. The purpose here is to classify the repository's remaining scene-document, `noon-ir`, JSON, and transport consumers so A4 can delete migration-only boundaries without accidentally deleting genuine external/cross-context codecs.

## Decision rule

Serialization is allowed only at an explicit external boundary:

- debug/export/import;
- persisted interchange;
- deterministic test fixtures/goldens;
- genuine cross-context transport, such as separate browser/Python workers.

It is not an ordinary boundary between Rust authoring, Semantic Scene, lowering, runtime, renderer, or a direct single-context Rust/WASM host.

A browser target alone is not a transport boundary.

## Current dependency shape

The engine core is already largely free of `noon-ir`:

| Consumer | `noon-ir` dependency | A4 classification | Direction |
| --- | --- | --- | --- |
| `crates/noon-compile` | none | typed engine layer | keep typed; no serialization |
| `crates/noon-runtime` | none | typed engine layer | keep typed; no serialization |
| `crates/noon` | dev-dependency only | tests/fixtures | remove as migration tests disappear |
| `crates/noon-web` | production dependency | migration + real browser transport mixed together | split migration codecs from genuine cross-context codecs |

This makes `noon-web` the current production concentration point for scene-document migration debt. A4 should avoid reintroducing `noon-ir` into compiler/runtime/public Rust layers while deleting it from web/platform paths.

## Consumer ledger

| Seam / consumer | Current use | Classification | Version compatibility requirement | Exit condition / owner |
| --- | --- | --- | --- | --- |
| `crates/noon-ir/src/{legacy,mixed,semantic*}.rs` | overlapping serializable scene/interchange families | migration model + codec crate | **not presumed**; greenfield internal formats are not compatibility commitments | #959/A4.5 decides whether any independent external interchange remains; otherwise delete crate after callers move |
| `crates/noon-web/src/retained_authoring_wire_scene.rs` | normalizes legacy geometry JSON + retained sidecar JSON into `SceneSpec`; exposes `canonicalRetainedSceneSpecJson` | migration-only producer/transport bridge | no product compatibility requirement identified | delete when producers construct shared semantic state directly and any real worker boundary has a narrow codec (#959 with #969 host work) |
| `crates/noon-web/src/retained_scene_spec_runtime.rs` | lowers `SceneSpec` by rebuilding a geometry `SceneDefinition`, then calling `RetainedScene::from_legacy`, then inserts retained text | migration-only in-process bridge | none | replace with direct Semantic Scene/lowering path; specifically remove `SceneSpec -> SceneDefinition -> RetainedScene::from_legacy` (#959/A4.2-A4.3) |
| `crates/noon/src/text_authoring.rs::RetainedScene::from_legacy` | lifts geometry-only `SceneDefinition` into retained compiler input | migration compatibility constructor | none | remove after production callers use Semantic Scene / target semantic object state; test-only callers must not keep it alive (#959) |
| `crates/noon-core/src/object_content.rs::{ObjectContentRef,RetainedObjectDefinition}` | legacy retained compiler/frontend payload | migration-only object model (explicitly marked #959-owned) | none | delete after compiler/frontend consumers move to target semantic content/state and lowering owns compact execution values (#959, dependent on A1 progress) |
| `crates/noon-web/src/execution_transport.rs` execution-delta envelope/codec | transfers runtime deltas for browser worker rendering and supports typed mirror application plus JSON wrappers | genuine cross-context transport **when crossing workers** | define only for the explicit worker protocol; do not promote it to engine ABI | retain/narrow codec at worker edge; direct Rust/WASM same-context path from #969 must bypass it |
| `web/execution-engine-worker.js` + render-worker `MessagePort`/shared/transferable transport | real Web Worker boundary between execution and rendering contexts | genuine cross-context transport | protocol versioning may be required for this boundary; scope it to worker messages, not semantic scene authority | keep transport only if architecture continues to use separate contexts; #969 owns direct Rust/WASM host path |
| `web/execution-engine-worker.js` `sceneJson` init/replace/reconcile + WASM `EngineScenePlayer` JSON APIs | serializes authored/scene state into the execution worker/WASM player | migration scene boundary mixed into worker control | no compatibility requirement identified for the scene document itself | replace with a narrow worker command/codec around the canonical semantic/execution representation, or bypass entirely for same-context host (#959 + #969) |
| Python/Pyodide worker <-> execution worker callback frames/patch batches | invokes arbitrary host-language callbacks across a real worker boundary | genuine cross-context host transport | version only the explicit host callback protocol if needed | retain narrow host callback codec; converge authored mutations on the shared semantic mutation vocabulary rather than a parallel scene patch authority (#957/#959) |
| `web/wire-contracts.test.mjs` and related wire/JSON fixtures | validates current serialized envelopes | test fixtures / migration contract tests | tests do not create a compatibility promise by themselves | keep only fixtures that protect an intentional external/worker boundary; delete migration-format goldens with their production seam (#959/A4.8) |
| Rust tests/examples/parity paths importing `noon-ir` | round-trip/compatibility proof around migration scene documents | test/example migration consumers | none unless tied to an explicitly retained external codec | migrate to typed semantic construction; retain serialized fixtures only for explicit codec tests (#959/A4.7-A4.8) |

## Important separation: real transport versus accidental engine boundary

The current web stack contains both categories and they must not be deleted or preserved as one unit.

A real worker edge can serialize:

```text
execution worker
  -> MessagePort / SharedArrayBuffer / transferable payload
  -> render worker
```

Likewise, a separate Python/Pyodide worker may exchange callback frames and mutation results with the execution worker.

But these are not evidence that the engine itself should be:

```text
Rust API
  -> JSON SceneSpec
  -> JS
  -> JSON SceneDefinition
  -> Rust/WASM runtime
```

The latter is migration debt. Direct native and direct Rust/WASM paths must stay typed and in-memory.

## First deletion targets

The safest deletion order from the current tree is:

1. **Stop creating new scene-document consumers.** `noon-core`, `noon-compile`, and `noon-runtime` must remain free of `noon-ir` dependencies.
2. **Remove the canonical retained lowering detour.** Replace the production `SceneSpec -> SceneDefinition -> RetainedScene::from_legacy` path in `retained_scene_spec_runtime.rs` once the target Semantic Scene/lowering input is available.
3. **Remove `RetainedScene::from_legacy` production use.** Test-only uses must migrate rather than extending this constructor's lifetime.
4. **Collapse web producer normalization.** Delete `legacy geometry + retained sidecar -> SceneSpec` once frontends target shared semantic operations directly.
5. **Narrow browser serialization to actual worker protocols.** Keep execution/host transport codecs only where a cross-context boundary still exists; #969's direct same-context Rust/WASM path must not require them.
6. **Delete overlapping `noon-ir` document families and then the crate if no independent interchange consumer remains.** Do not keep it merely because migration tests import it.
7. **Delete obsolete fixtures/docs together with each seam.** A golden file is not a reason to preserve the format it tests.

## A4 code-search guard list

Before each A4 deletion PR, rerun these searches from repository root and classify every new hit. They are intentionally source-oriented rather than a frozen allow-list; the desired direction is fewer hits.

```bash
# Crate/dependency ownership
git grep -nE 'noon[-_]ir' -- 'Cargo.toml' '*.rs'

# Overlapping authored scene/document models
git grep -nE '\b(SceneSpec|SceneDefinition|SemanticSceneSpec|Mixed.*Scene)\b' -- '*.rs' '*.js' '*.mjs' '*.py'

# Legacy lifting/adapters
git grep -nE '\bfrom_legacy\b|legacy_scene|retained.*sidecar|decode_scene|encode_scene' -- '*.rs' '*.js' '*.mjs' '*.py'

# Scene serialization used as execution plumbing
git grep -nE 'sceneJson|scene_json|scene.*to_json|scene.*from_json' -- '*.rs' '*.js' '*.mjs' '*.py'

# Explicit browser/host transport: classify, do not blindly delete
git grep -nE 'MessagePort|postMessage|SharedArrayBuffer|patchBatchJson|deltaJson|protocolVersion' -- 'web/**' 'crates/noon-web/**'
```

For every hit, record one of:

- `typed engine` — serialization forbidden;
- `migration-only` — #959 deletion owner and concrete removal condition required;
- `test fixture` — must disappear with the migration seam unless it validates a retained codec;
- `debug/export/persisted interchange` — explicit product boundary required;
- `cross-context transport` — allowed only at the actual process/worker boundary.

## Ratchet handoff

A6/#961 should tighten absence checks only after an A4 deletion lands. Useful ratchets include:

- no new `noon-ir` dependency outside the current migration owner;
- no `SceneSpec`/`SceneDefinition` use in compiler/runtime/renderer layers;
- no `from_legacy` production calls once its final caller is removed;
- no scene JSON methods on the direct native or direct Rust/WASM engine path;
- no worker transport type becoming a Semantic Scene/runtime authority.

The ratchet should protect a deletion, not grandfather a new compatibility surface.

## Inventory limitation

GitHub's repository code-search index was unavailable while this inventory was prepared, so this document deliberately does **not** claim that the table is a complete symbol-level enumeration of every text hit. The dependency graph, current production ownership islands, and concrete migration seams above were inspected directly from the current repository tree. A4 deletion PRs must rerun the guard list locally (or with a restored code index) and update this ledger if additional consumers are found.
