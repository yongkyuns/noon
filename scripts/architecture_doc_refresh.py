#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_between(text: str, start: str, end: str, replacement: str) -> str:
    i = text.index(start)
    j = text.index(end, i + len(start))
    return text[:i] + replacement + text[j:]


def replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise AssertionError(f"expected one occurrence, found {text.count(old)}: {old[:80]!r}")
    return text.replace(old, new, 1)


# README: orient readers quickly; implementation detail stays in architecture.md and examples.
readme_path = ROOT / "README.md"
readme = readme_path.read_text()
readme = replace_between(
    readme,
    "# Noon\n\n",
    "## Architecture\n",
    "# Noon\n\n"
    "Noon is a **Rust-native 2D animation and interactive graphics engine** with "
    "Manim-compatible Python authoring, built around a shared semantic scene, a "
    "deterministic runtime, and a retained GPU renderer.\n\n"
    "Rust and Python expose the same observable scene semantics through shared Rust "
    "operations. Python supplies Manim-compatible syntax and arbitrary host callbacks "
    "where they are genuinely required; it does not implement a second scene, "
    "scheduler, runtime, or renderer.\n\n",
)
readme = replace_between(
    readme,
    "## Architecture\n",
    "## Authoring\n",
    "## Architecture\n\n"
    "The main data path is intentionally small. Focused diagrams carry the detailed "
    "publication, locality, callback, deployment, and ownership contracts instead of "
    "crowding them into one picture.\n\n"
    "![Horizontal architecture: authoring feeds the semantic scene, lowering produces CompiledScene, runtime publishes frame changes to the renderer, and host services supply input/ticks and device/presentation.](docs/diagrams/overview.svg)\n\n"
    "[D2 source](docs/diagrams/overview.d2) · "
    "[Domain projections](docs/architecture.md#domain-projections) · "
    "[Revision and lifetime model](docs/architecture.md#identity-generations-revisions-versions-and-sequences) · "
    "[Locality propagation](docs/architecture.md#runtime-complexity-contract) · "
    "[Callback transaction](docs/architecture.md#ordered-transactional-host-callback-overlay) · "
    "[Current ownership](docs/architecture.md#current-implementation-ownership) · "
    "[Python worker topology](docs/architecture.md#host-language-or-multi-worker-topology).\n\n"
    "The overview separates responsibilities rather than advertising another framework:\n\n"
    "- **Authoring** supplies Rust/Python ergonomics and invokes shared semantic operations.\n"
    "- **Semantic scene** (`SemanticStore`) owns authored identity, structure, declarations, and persistent state.\n"
    "- **Lowering** (`noon-compile`) derives replaceable execution data such as slots, tracks, reactive dependencies, and resource projections.\n"
    "- **Runtime/session** owns effective time-varying state, ordering, completion, wake/sleep, and coherent publication.\n"
    "- **Renderer** owns retained GPU resources and dirty draw work only; native/browser hosts own platform lifecycle.\n\n"
    "Normal native and single-context Rust/WASM engine boundaries are typed in-process "
    "Rust boundaries. Serialization is reserved for explicit codecs or genuine "
    "cross-context transport. The [architecture guide](docs/architecture.md) is the "
    "single normative source for the contracts and roadmap.\n\n",
)
readme = replace_between(
    readme,
    "### Ordinary API versus integration\n",
    "Equivalent examples run through the native Rust renderer and the Python browser host:\n",
    "### Live authoring and integration boundaries\n\n"
    "The crate root and `noon::prelude` expose ordinary authoring handles, values, live "
    "operations, completion, and typed errors. `noon::integration` is the explicit raw "
    "semantic/resource and host plumbing boundary; `noon::diagnostics` is opt-in "
    "debug/export access. Neither namespace creates another scene or runtime.\n\n"
    "After creating an execution session, `scene.live(&mut session)` applies supported "
    "persistent edits and animation/completion operations through the same staged "
    "semantic/execution publication path. Rust `Mobject` inspection reads authored/base "
    "state; live/effective queries read the latest coherent runtime state. Failed "
    "preparation leaves both authored and effective published state unchanged.\n\n"
    "Raw integration-store access is deliberately not a live-mutation shortcut: edits "
    "outside coherent publication can stale a session and must be handled explicitly. "
    "See [`shared_authoring.rs`](crates/noon/examples/shared_authoring.rs) for the public "
    "typed path and `docs/architecture.md` for the authored/effective and publication "
    "contracts.\n\n"
    "Equivalent examples run through the native Rust renderer and the Python browser host:\n",
)
readme = replace_between(
    readme,
    "Equivalent examples run through the native Rust renderer and the Python browser host:\n",
    "Run a Rust example with",
    "Equivalent examples run through the native Rust renderer and the Python browser host:\n\n"
    "| Feature | Rust | Python |\n"
    "| --- | --- | --- |\n"
    "| Geometry and text | [shared_text.rs](crates/noon-native/examples/shared_text.rs) | [shared_text.py](web/python/examples/shared_text.py) |\n"
    "| Live membership | [live_semantic_scene.rs](crates/noon-native/examples/live_semantic_scene.rs) | [live_semantic_scene.py](web/python/examples/live_semantic_scene.py) |\n"
    "| Ordinary affine playback | [ordinary_affine_play.rs](crates/noon-native/examples/ordinary_affine_play.rs) | [ordinary_affine_play.py](web/python/examples/ordinary_affine_play.py) |\n"
    "| Composition | [ordinary_composition_play.rs](crates/noon-native/examples/ordinary_composition_play.rs) | [ordinary_composition_play.py](web/python/examples/ordinary_composition_play.py) |\n"
    "| Ordered callbacks | [live_affine_callbacks.rs](crates/noon-native/examples/live_affine_callbacks.rs) | [live_affine_callbacks.py](web/python/examples/live_affine_callbacks.py) |\n"
    "| Content replacement | [live_content_switch.rs](crates/noon-native/examples/live_content_switch.rs) | [live_content_switch.py](web/python/examples/live_content_switch.py) |\n\n"
    "The complete qualification corpus lives under [`crates/noon-native/examples`](crates/noon-native/examples) "
    "and [`web/python/examples`](web/python/examples).\n\n"
    "Run a Rust example with",
)
readme = replace_between(
    readme,
    "The callback examples run forward",
    "## Browser playground\n",
    "The broader example corpus covers callback ordering, sparse reads, family operations, "
    "composition, transforms, text, and renderer behavior. Required host callbacks hold "
    "authored progress at their ordered barrier; deterministic segments do not require "
    "per-frame Python execution when no host-dynamic work is scheduled. Unsupported "
    "compatibility behavior remains explicit rather than silently approximated.\n\n"
    "Current Phase A acceptance status is intentionally not duplicated here; use the "
    "[Phase A umbrella](https://github.com/yongkyuns/noon/issues/953) and the architecture "
    "guide for current ownership and invariants.\n\n"
    "## Browser playground\n",
)
readme = replace_between(
    readme,
    "`noon-ir` and the obsolete browser scene/execution mirrors have been deleted.",
    "Crates should correspond to real dependency or compilation boundaries.",
    "`noon-ir`, migration scene/document models, and obsolete browser execution mirrors "
    "have been deleted. Module/crate normalization is complete: `SemanticStore` remains "
    "in `noon-core`, `CompiledScene` in `noon-compile`, effective execution in "
    "`noon-runtime`, and session orchestration in `noon`. Serialization remains only for "
    "explicit codecs and genuine cross-context transport.\n\n"
    "The [current ownership diagram](docs/architecture.md#current-implementation-ownership) "
    "shows these settled responsibility boundaries. Remaining finite Phase A acceptance "
    "work is tracked from [#953](https://github.com/yongkyuns/noon/issues/953), rather than "
    "by reopening completed migration/module-normalization tracks. Diagram sources and "
    "checked-in SVGs are refreshed with `python3 scripts/architecture_diagrams.py`; CI "
    "checks that they agree.\n\n"
    "Crates should correspond to real dependency or compilation boundaries.",
)
readme = replace_between(
    readme,
    "## Design priorities\n",
    "Compatibility is an API/semantic goal",
    "## Design priorities\n\n"
    "In order:\n\n"
    "1. one authoritative Semantic Scene and one typed lowering/publication boundary;\n"
    "2. first-class Rust authoring and execution on native and direct Rust/WASM targets;\n"
    "3. Manim-compatible Python ergonomics over the same shared semantics;\n"
    "4. unrestricted interactivity and mutability without imposing dynamic overhead on static content;\n"
    "5. deterministic correctness, direct-seek semantics, and coherent authored/effective state;\n"
    "6. automatic specialization and strict locality through validation, execution, rendering, and GPU publication;\n"
    "7. explicit, measured deviations only where exact Manim behavior has a fundamental design or performance blocker.\n\n"
    "Compatibility is an API/semantic goal",
)
readme_path.write_text(readme)


# Architecture guide: distinguish normative architecture, current placement, and mutable roadmap status.
arch_path = ROOT / "docs/architecture.md"
arch = arch_path.read_text()
arch = replace_once(
    arch,
    "Detailed subsystem documents may explain an implementation, test strategy, or compatibility behavior, but they do not define a second architecture or roadmap.\n",
    "Detailed subsystem documents may explain an implementation, test strategy, or compatibility behavior, but they do not define a second architecture or roadmap.\n\n"
    "Read this document in three modes:\n\n"
    "- **Normative architecture** defines ownership, interfaces, ordering, publication, locality, and lifetime rules.\n"
    "- **Current implementation** callouts describe where those responsibilities live today without creating a second target architecture.\n"
    "- **Roadmap status** is intentionally coarse; mutable task inventories and final qualification evidence live in GitHub issues, primarily the Phase A umbrella.\n",
)
arch = replace_between(
    arch,
    "### Rust-native product invariant\n",
    "Python and JavaScript/TypeScript are optional language adapters",
    "### Direct Rust execution invariant\n\n"
    "Native Rust and Rust compiled to WASM use the same typed engine path; only the "
    "platform shell changes:\n\n"
    "![One shared typed Rust engine feeds either the native platform host or the browser/WASM platform host; no transport sits between the in-process engine layers.](diagrams/direct-execution-hosts.svg)\n\n"
    "[D2 source](diagrams/direct-execution-hosts.d2).\n\n"
    "The shared path is `Rust API -> Semantic Scene -> Execution Plan -> Runtime -> "
    "Renderer`. When those layers share one process or one WASM execution context, every "
    "boundary is typed and in memory.\n\n"
    "#### Native host\n\n"
    "A native Rust application can author, lower, execute, and render without Python, "
    "JavaScript, WASM, a browser runtime, JSON, serialization/deserialization, or a "
    "host-language bridge between engine layers. `noon-native` owns window/event-loop, "
    "surface, input, acquire/submit/present, and recovery mechanics at the platform edge.\n\n"
    "#### Browser/WASM host\n\n"
    "A Rust application compiled to WASM uses the same semantic, compiler, runtime, and "
    "renderer responsibilities in one WASM context. JavaScript may bootstrap the module, "
    "supply the canvas, and provide browser lifecycle glue, but it does not receive and "
    "re-send scene/runtime state between Rust layers. A browser target by itself is not a "
    "transport boundary.\n\n"
    "Serialized transport is justified only at a real external or cross-context boundary, "
    "for example the current Python authoring context sending derived resources/deltas to "
    "a separate render owner; see section 11.\n\n"
    "Python and JavaScript/TypeScript are optional language adapters",
)
arch = replace_once(
    arch,
    "[D2 source](diagrams/overview.d2) · [Domain projections](#domain-projections) · [Current crate ownership](#current-implementation-ownership).",
    "[D2 source](diagrams/overview.d2) · [Domain projections](#domain-projections) · [Revision/lifetime model](#identity-generations-revisions-versions-and-sequences) · [Locality propagation](#runtime-complexity-contract) · [Callback transaction](#ordered-transactional-host-callback-overlay) · [Current crate ownership](#current-implementation-ownership).",
)
arch = replace_between(
    arch,
    "### Live-session control plane\n",
    "---\n\n## 3. Semantic Scene",
    "### Live-session control plane\n\n"
    "A continuously live authoring experience requires coordination, but coordination is "
    "not another state authority. `ExecutionSession`/`LiveProgram` may order script "
    "continuations, wake/sleep, input delivery, callback barriers, revision checks, and "
    "publication receipts while semantic truth stays in the Semantic Scene and effective "
    "time-varying truth stays in the Runtime.\n\n"
    "The [host-continuation view](#host-language-execution-invariant) shows how different "
    "host execution models converge on the same logical completion contract. The "
    "[live-publication view](#effective-driver-writes-vs-authored-semantic-mutations) shows "
    "how staged authored/effective work becomes one coherent publication, and the "
    "[callback transaction view](#ordered-transactional-host-callback-overlay) shows the "
    "ordered host barrier. These focused views replace a separate control-plane diagram "
    "that otherwise duplicated the same relationships.\n\n"
    "`play()`/`wait()`-class operations are logical segment-completion barriers, not "
    "necessarily blocking function calls and not exclusive interaction modes. Python may "
    "suspend its authoring worker; native Rust may drive/await compiled control flow; "
    "browser/WASM code yields rather than blocking the event loop.\n\n"
    "---\n\n## 3. Semantic Scene",
)
arch = replace_once(
    arch,
    "`noon-core` should converge on this normalized execution-level responsibility. Authoring compatibility helpers do not belong there.",
    "Execution-plan ownership remains below the semantic store: `noon-core` owns shared semantic identity/store and renderer-independent contracts, while `noon-compile` owns `CompiledScene` and lowering/specialization. Authoring compatibility helpers belong in the public/frontend layers, not in either shared engine contract merely for convenience.",
)
arch = replace_between(
    arch,
    "### Identity generations, revisions, versions and sequences\n",
    "### Runtime complexity contract\n",
    "### Identity generations, revisions, versions and sequences\n\n"
    "Identity validity, authored/execution revisioning, effective publication, external "
    "event ordering, and GPU lifetime are distinct domains. They must not collapse into one "
    "ambiguous global generation counter:\n\n"
    "![SceneRevision derives an ExecutionRevision, FrameEpoch publishes compatible effective state, ResourceVersion participates in publication and SubmissionSerial governs GPU retirement; identity/input/callback domains remain distinct.](diagrams/revision-publication-lifetime.svg)\n\n"
    "[D2 source](diagrams/revision-publication-lifetime.d2).\n\n"
    "| Domain | Meaning |\n"
    "| --- | --- |\n"
    "| `NodeId` / `ExecutionSlotId` generation | Identity validity across slot reuse; stale handles cannot alias replacements. |\n"
    "| `SceneRevision` | One coherently committed authored semantic-scene revision. |\n"
    "| `ExecutionRevision` | One compatible derived execution projection. |\n"
    "| `FrameEpoch` | One coherent effective runtime/presentation publication referencing a scene/execution revision pair. |\n"
    "| `ResourceVersion` | Immutable content/resource replacement version. |\n"
    "| `InputSequence` | Ordered external event sequence. |\n"
    "| `CallbackEpoch` | Ordered callback/evaluation request/result context. |\n"
    "| `SubmissionSerial` | GPU submission/fence/retirement ordering. |\n\n"
    "Values from different domains are not directly comparable merely because they are "
    "integers. Async/callback/resource results carry the exact identity/revision/version "
    "context needed to prove applicability. Consumers may remember the last relevant "
    "revision/version rather than relying on global dirty-bit clearing; late results must "
    "be rejected or reconciled deterministically instead of overwriting newer state.\n\n"
    "### Runtime complexity contract\n",
)
arch = replace_once(
    arch,
    "Arbitrary source-language re-execution is another explicit exception: Noon cannot promise sublinear execution of arbitrary Python/Rust/JS program logic. Hot-reload reconciliation must still ensure that unchanged semantic/execution/runtime/renderer state is preserved and that re-executed authoring work does not imply whole-scene lowering or GPU replacement.\n",
    "Arbitrary source-language re-execution is another explicit exception: Noon cannot promise sublinear execution of arbitrary Python/Rust/JS program logic. Hot-reload reconciliation must still ensure that unchanged semantic/execution/runtime/renderer state is preserved and that re-executed authoring work does not imply whole-scene lowering or GPU replacement.\n\n"
    "The expected propagation of a local change is therefore explicit:\n\n"
    "![A local authored edit is impact-analyzed and prepared locally, while effective-only writes join at affected runtime state; only affected spatial/publication/render ranges and GPU uploads change, and unrelated state remains resident.](diagrams/locality-propagation.svg)\n\n"
    "[D2 source](diagrams/locality-propagation.d2). Effective-only driver writes bypass "
    "authored relowering and join at affected runtime state. Authored structural/resource "
    "changes perform only the required local preparation. Unrelated semantic identities, "
    "execution slots, retained resources, and GPU ranges remain untouched unless observable "
    "semantics genuinely require wider materialization.\n",
)
arch = replace_between(
    arch,
    "Conceptually, a host barrier in an ordered evaluation plan behaves as:\n",
    "If compatibility semantics require updater order",
    "The ordered callback barrier is a transaction, not a sequence of scalar bridge calls:\n\n"
    "![Sequence: runtime pins a coherent callback read view, host code reads through an overlay and stages effective/authored writes, compiler/runtime prepare fallible work, then one new FrameEpoch publishes atomically.](diagrams/callback-transaction.svg)\n\n"
    "[D2 source](diagrams/callback-transaction.d2). The callback invocation is pinned to "
    "one coherent scene/execution/frame context. Reads consult pending overlay writes first "
    "and then the revision-pinned read view; writes accumulate in one `StagedUpdateBatch` as "
    "effective driver writes and/or authored semantic mutations. Fallible semantic/resource "
    "preparation completes before the next publication becomes visible.\n\n"
    "If compatibility semantics require updater order",
)
arch = replace_once(
    arch,
    "See the [direct Rust/WASM diagram](#rust-on-web-product-invariant) ([D2 source](diagrams/wasm-execution.d2)). The same four engine authorities execute in a single WASM context; only platform lifecycle crosses into JavaScript.",
    "See the [direct Rust execution diagram](#direct-rust-execution-invariant) ([D2 source](diagrams/direct-execution-hosts.d2)). The same semantic/compiler/runtime/renderer responsibilities execute in a single WASM context; only platform lifecycle crosses into JavaScript.",
)
arch = replace_once(
    arch,
    "The [native path](#rust-native-product-invariant) and [direct Rust/WASM path](#rust-on-web-product-invariant) diagrams show the platform shells. Both hosts own surface/device/queue configuration, resize and input ingress, and acquire/submit/present/recovery policy. They reuse `noon-runtime` and `noon-render-wgpu`; neither owns another scene or scheduler.",
    "The [direct Rust execution diagram](#direct-rust-execution-invariant) shows both platform shells around the same typed engine. Both hosts own surface/device/queue configuration, resize and input ingress, and acquire/submit/present/recovery policy. They reuse `noon-runtime` and `noon-render-wgpu`; neither owns another scene or scheduler.",
)
arch = replace_between(
    arch,
    "## 13. Crate and module boundaries\n",
    "---\n\n## 14. Correctness invariants",
    "## 13. Crate and module boundaries\n\n"
    "Crates exist only for real dependency, compilation-target, or reuse boundaries. The "
    "Phase A5 normalization is complete, so current ownership and the settled responsibility "
    "model are now the same architecture rather than a temporary map waiting to be renamed.\n\n"
    "### Current implementation ownership\n\n"
    "![Current ownership: noon provides authoring and session coordination; noon-core holds semantic storage and shared contracts; noon-compile lowers and owns CompiledScene; noon-runtime executes; renderer and platform hosts remain separate.](diagrams/crate-ownership.svg)\n\n"
    "[D2 source](diagrams/crate-ownership.d2). This is an ownership/data-flow view, not a "
    "complete Cargo dependency graph. Refresh it when a responsibility boundary changes; do "
    "not pin it to a historical inspection SHA.\n\n"
    "Settled responsibilities are:\n\n"
    "```text\n"
    "noon\n"
    "  public Rust authoring facade and shared high-level operations\n"
    "  ExecutionSession / LiveSession orchestration and explicit integration surfaces\n\n"
    "noon-core\n"
    "  SemanticStore, semantic identity and authored declarations / transactions\n"
    "  shared renderer-independent resources and engine contracts\n\n"
    "noon-compile\n"
    "  SemanticStore -> derived execution projection\n"
    "  CompiledScene, specialization, execution mapping and publication preparation\n\n"
    "noon-runtime\n"
    "  SceneInstance, effective state, timeline/reactive evaluation, spatial state\n"
    "  scheduling, revisions and runtime publication\n\n"
    "noon-render-wgpu\n"
    "  retained GPU resources, preparation and draw encoding for native/web reuse\n\n"
    "noon-native\n"
    "  native window/event-loop/input/surface/presentation dependency boundary only\n\n"
    "noon-web\n"
    "  WASM/browser bindings, canvas/frame/input integration and explicit worker transport\n"
    "  direct single-context Rust/WASM execution remains typed in process\n\n"
    "noon-geometry / noon-text / noon-typst\n"
    "  supporting provider boundaries justified by reusable algorithms or heavy dependencies\n"
    "```\n\n"
    "`SemanticStore` deliberately remains in `noon-core`: compiler/runtime consumers must "
    "not acquire an upward dependency on the public facade merely to make the conceptual "
    "Semantic Scene share the `noon` crate name. `CompiledScene` remains compiler-owned, "
    "effective state remains runtime-owned, and session orchestration remains in `noon`.\n\n"
    "Rules:\n\n"
    "- do not reintroduce `noon-ir`, migration scene/document models, or normal-path serialized engine bridges;\n"
    "- no crate exists solely for migration compatibility or naming symmetry;\n"
    "- module structure must expose ownership directly rather than hide unrelated domains behind `#[path]`/organizational `include!`;\n"
    "- native platform dependencies stay outside reusable semantic/compiler/runtime/renderer crates;\n"
    "- prefer modules over crates until an actual dependency, compilation, or reuse boundary exists;\n"
    "- conceptual names such as ECS/world/session/scheduler do not justify a public crate or new state authority.\n\n"
    "---\n\n## 14. Correctness invariants",
)
arch = replace_between(
    arch,
    "## Phase A — architecture consolidation\n",
    "---\n\n## Phase B — complete common 2D semantics",
    "## Phase A — architecture consolidation\n\n"
    "**Status: the architecture foundations are implemented; Phase A remains open only for "
    "finite acceptance/qualification closeout tracked in [#953](https://github.com/yongkyuns/noon/issues/953).** "
    "Correctness fixes may proceed at any time; broad feature expansion does not become the "
    "default priority until the umbrella exit gate closes.\n\n"
    "The permanent Phase A result is already reflected in the normative sections above:\n\n"
    "- one authoritative `SemanticStore` / semantic identity space and one persistent mutation vocabulary;\n"
    "- one typed semantic-to-execution lowering/publication path with impact-local preparation;\n"
    "- first-class Rust authoring plus direct typed native and single-context Rust/WASM execution;\n"
    "- one `ExecutionSession` orchestration surface over existing compiler/runtime ownership;\n"
    "- authored/base state distinct from effective Runtime state and coherent `FrameEpoch` publication;\n"
    "- migration scene/IR/sidecar architecture deleted and serialization limited to explicit codecs or real context boundaries;\n"
    "- settled module/crate ownership and structural architecture ratchets;\n"
    "- paired Rust/Python and deterministic execution evidence for representative supported semantics.\n\n"
    "Completed migration/module-normalization tracks are historical evidence, not unfinished "
    "target architecture: A4 (#959), A5 (#960), and the A6 ratchet framework (#961) are "
    "closed. The mutable finite closeout list—currently centered on the remaining thin "
    "Python-facade acceptance and final architecture-gate/browser evidence—belongs in #953 "
    "and its linked work, not in this permanent document. Do not infer a need to replay or "
    "reopen completed migration tracks from the existence of Phase A.\n\n"
    "**Phase A exit:** the #953 checklist is completely green with no unresolved correctness "
    "failure; then Phase B breadth becomes the default priority.\n\n"
    "---\n\n## Phase B — complete common 2D semantics",
)

# Update the crate ownership diagram note to match the settled A5 decision.
crate_d2 = ROOT / "docs/diagrams/crate-ownership.d2"
crate_text = crate_d2.read_text()
crate_text = replace_once(
    crate_text,
    "The target crate split below is not a completed module move.",
    "These are settled responsibility boundaries; conceptual engine layers do not require matching crate names.",
)
crate_d2.write_text(crate_text)

# New focused diagrams.
diagrams = ROOT / "docs/diagrams"
(diagrams / "direct-execution-hosts.d2").write_text(r'''# Native and direct Rust/WASM use the same typed engine; only the platform shell changes.
...@_style

direction: down
app: "Compiled Rust application\nnative binary or Rust/WASM" {class: node}
engine: "SHARED TYPED RUST ENGINE" {
  grid-columns: 4
  horizontal-gap: 54
  scene: "Semantic Scene\nSemanticStore" {class: authority}
  plan: "Execution Plan\nnoon-compile" {class: authority}
  runtime: "Runtime\nnoon-runtime" {class: authority}
  renderer: "Retained renderer\nnoon-render-wgpu" {class: authority}
  scene -> plan: "typed lowering"
  plan -> runtime: "typed execution data"
  runtime -> renderer: "FrameEpoch / publication"
}
hosts: "PLATFORM EDGE · deployment alternatives" {
  grid-columns: 2
  horizontal-gap: 150
  native: "noon-native\nwinit · input · surface\nacquire / submit / present" {class: host}
  web: "noon-web + browser glue\nWASM bootstrap · input · canvas\nWebGPU / WebGL2 presentation" {class: host}
}
app -> engine.scene: "author / mutate"
engine.renderer -> hosts.native: "native surface"
engine.renderer -> hosts.web: "browser canvas"
note: "No JSON, scene document or language bridge sits between the four engine responsibilities.\nHosts drive input/frame lifecycle but own no semantic or runtime truth." {class: note}
hosts -> note: {style.opacity: 0}
''')

(diagrams / "revision-publication-lifetime.d2").write_text(r'''# Distinct identity/revision/lifetime domains; not one global generation counter.
...@_style

direction: down
main: "COHERENT STATE / LIFETIME PATH" {
  grid-columns: 4
  horizontal-gap: 72
  scene: "SceneRevision\ncommitted authored state" {class: authority}
  execution: "ExecutionRevision\nderived projection" {class: authority}
  frame: "FrameEpoch\neffective publication" {class: authority}
  submit: "SubmissionSerial\nGPU lifetime / retirement" {class: node}
  scene -> execution: "derive / prepare"
  execution -> frame: "evaluate / publish"
  frame -> submit: "submit referenced resources"
}
contexts: "OTHER DISTINCT DOMAINS" {
  grid-columns: 4
  horizontal-gap: 38
  identity: "NodeId generation\nidentity validity" {class: node}
  resource: "ResourceVersion\nimmutable content" {class: node}
  input: "InputSequence\nordered ingress" {class: node}
  callback: "CallbackEpoch\nordered host context" {class: node}
}
contexts.identity -> main.scene: "valid handles" {style.stroke-dash: 4}
contexts.resource -> main.frame: "referenced version" {style.stroke-dash: 4}
contexts.input -> main.frame: "staged event context" {style.stroke-dash: 4}
contexts.callback -> main.frame: "apply only if current" {style.stroke-dash: 4}
note: "Different domains are never compared merely because they are integers.\nLate async/callback/resource results carry the exact context needed to prove applicability." {class: note}
main -> note: {style.opacity: 0}
''')

(diagrams / "locality-propagation.d2").write_text(r'''# A local change stays local through validation, execution, rendering and GPU publication.
...@_style

direction: right
sources: "CHANGE SOURCE" {
  grid-columns: 1
  vertical-gap: 34
  authored: "Local authored edit\nproperty · structure · content" {class: node}
  effective: "Effective driver write\ntimeline · reactive · callback" {class: node}
}
impact: "Impact + preparation\naffected closure only\ncompiler/resource work when required" {class: node}
runtime: "Affected runtime state\nslots · domains · spatial entries" {class: authority}
publish: "RendererPublication\nchanged ranges · order · resources" {class: node}
renderer: "Retained renderer\nreuse clean resident state" {class: authority}
gpu: "GPU work\nchanged uploads + visible draw work" {class: node}

sources.authored -> impact: "validate / classify"
impact -> runtime: "prepared local effects"
sources.effective -> runtime: "no authored relowering"
runtime -> publish: "coherent FrameEpoch"
publish -> renderer: "dirty projection"
renderer -> gpu: "upload / encode"
note: "Unrelated NodeIds, execution slots, resources and GPU ranges remain untouched.\nWider materialization occurs only when observable semantics genuinely require it." {class: note}
gpu -> note: {style.opacity: 0}
''')

(diagrams / "callback-transaction.d2").write_text(r'''# One ordered callback invocation with a coherent read view and staged overlay.
shape: sequence_diagram
runtime: "Runtime"
view: "Callback read view"
host: "Host callback"
overlay: "Staged overlay"
prepare: "Semantic / compiler preflight"

runtime -> view: "pin SceneRevision / ExecutionRevision / FrameEpoch"
view -> host: "invoke with required coherent reads"
host -> overlay: "read overlay first; stage effective/authored writes"
host -> view: "read miss: suspend/resume same invocation when supported"
overlay -> prepare: "authored mutations / resource work"
prepare -> runtime: "typed prepared effects or rejection"
overlay -> runtime: "effective writes"
runtime -> runtime: "atomic next FrameEpoch publication"
''')

# Retire duplicated / superseded views. SVGs are removed before regeneration.
for stem in ("native-execution", "wasm-execution", "session-control"):
    for suffix in (".d2", ".svg"):
        path = diagrams / f"{stem}{suffix}"
        if path.exists():
            path.unlink()

# Remove stale links/names and prove the intended architecture references are present.
for stale in (
    "native-execution.d2", "native-execution.svg", "wasm-execution.d2", "wasm-execution.svg",
    "session-control.d2", "session-control.svg", "#rust-native-product-invariant", "#rust-on-web-product-invariant",
    "checked against master `d6734d2a0626f5c9926e4043ad2017a94f5c8c5a`",
    "#960 tracks the remaining separation", "owned for deletion by #959",
    "`noon-core` should converge on this normalized execution-level responsibility",
):
    assert stale not in arch + readme, stale
for required in (
    "direct-execution-hosts.svg", "revision-publication-lifetime.svg",
    "locality-propagation.svg", "callback-transaction.svg",
):
    assert required in arch + readme, required

arch_path.write_text(arch)
print("architecture documentation refresh applied")
