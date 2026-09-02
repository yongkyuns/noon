# Noon architecture and roadmap

## Status

This document is the single authoritative architecture and roadmap for Noon.

Noon is a greenfield project. There is no requirement to preserve legacy Noon APIs, internal scene models, wire formats, compatibility aliases, migration adapters, or crate boundaries. If an old abstraction conflicts with this document, remove it rather than adapting around it. Git history is the archive.

Detailed subsystem documents may explain an implementation, test strategy, or compatibility behavior, but they do not define a second architecture or roadmap.

---

## 1. Product target

Noon is a high-performance animation and interactive graphics system with:

- a first-class idiomatic Rust authoring API that can run end-to-end in a native Rust process;
- a built-in renderer/runtime shared by native and web targets;
- Manim-compatible Python authoring for supported common 2D behavior as a wrapper over shared Rust semantics;
- optional future JavaScript/TypeScript authoring as another wrapper over shared Rust semantics;
- deterministic offline and realtime execution;
- native reactive interaction without requiring Python on the frame path;
- explicit support for arbitrary host-language callbacks when user code genuinely requires them;
- retained, incremental GPU rendering that scales with changed and visible work rather than total scene size.

### Rust-native product invariant

The complete native Rust path is a direct, typed, in-process Rust pipeline:

```text
Rust application
    |
    v
idiomatic `noon` Rust API
    |
    v
Semantic Scene
    |
    v
Execution Plan
    |
    v
Runtime
    |
    v
Renderer
    |
    v
native surface / GPU
```

This path must not require Python, JavaScript, WASM, a browser runtime, JSON, serialization/deserialization, transport documents, or a host-language bridge between these layers.

Python and JavaScript/TypeScript are optional language adapters over the same Rust semantic operations. They are not required components of the Rust engine, runtime, or renderer.

Manim Community v0.21.x is the compatibility oracle for supported common 2D Python behavior. Compatibility is a semantic/API goal, not an implementation constraint. Noon does not copy Manim's renderer, Python scene engine, internal point representation, or per-frame execution model.

---

## 2. Architecture in one picture

```text
                  Manim-compatible Python      future JS/TS
                            |                       |
                            +---- thin wrappers ----+
                                      |
                                      v
                         shared Rust semantic operations
                                      ^
                                      |
                            idiomatic Rust API
                                      |
                                      v
                              +-----------------------+
                              |    Semantic Scene     |
                              |  one source of truth  |
                              +-----------+-----------+
                                          |
                                  analyze / lower
                                          |
                                          v
                              +-----------------------+
                              |    Execution Plan     |
                              | compact + specialized |
                              +-----------+-----------+
                                          |
                                     instantiate
                                          |
                                          v
                              +-----------------------+
                              |        Runtime        |
                              | time + mutable state  |
                              | dirty/local updates   |
                              +-----------+-----------+
                                          |
                                          v
                              +-----------------------+
                              |       Renderer        |
                              | retained GPU state    |
                              +-----------------------+
                                   /             \
                              native surface      web surface
```

There are four engine layers and exactly one authority at each layer:

1. **Semantic Scene** — what the program means.
2. **Execution Plan** — the cheapest representation that preserves that meaning.
3. **Runtime** — the current execution state.
4. **Renderer** — a projection of runtime state into retained GPU resources and draw work.

The Rust public API is the first-class native authoring API for these layers. Python and future JS/TS adapt language syntax and host callbacks onto the same Rust semantic operations; they do not define separate engine layers.

Serialization is not a fifth scene model. It is an optional codec around one of these representations for explicit external boundaries such as export/import, debugging, tests, persistence, or unavoidable cross-context transport.

**Normal in-process engine boundaries are typed Rust boundaries.** `Rust API -> Semantic Scene -> Execution Plan -> Runtime -> Renderer` must not serialize to JSON or any other wire representation as part of ordinary native authoring, lowering, execution, mutation, or rendering.

---

## 3. Semantic Scene

The Semantic Scene is the only authored scene model.

It is mutable, hierarchical, language-neutral, and expressive enough to represent author intent before optimization.

It owns:

- stable generational node identity;
- detached versus scene-owned objects;
- source identity for hot reload/reconciliation;
- content: geometry, text, images, future meshes and other resources;
- high-precision semantic transforms and styles;
- family/group membership and ordering, including aliasing where Manim semantics require it;
- bounds and layout semantics;
- object lifecycle and scene membership;
- target-state animation semantics;
- animation trees, options, defaults, composition and scheduling intent;
- signals, trackers, bindings and derived expressions;
- updater and event registrations;
- host callback slots;
- camera semantics;
- mutations that change authored structure or properties.

Language wrappers hold stable handles into this scene. They do not own a parallel scene representation.

### Required semantic identity model

Use one scene-global generational `NodeId` (or equivalent) for semantic objects/families. Execution indices, GPU indices, resource handles and transport IDs are derived identities and must never become semantic authority.

### Required value model

Semantic values must not be constrained by the current 2D/f32 renderer. The semantic layer should use the representation needed by the product direction, including 3D-capable/high-precision transforms where appropriate, and lower explicitly to compact execution/render values.

Do not maintain permanent parallel `legacy` and `semantic` transform/style/object models.

---

## 4. Language frontends

High-level behavior is implemented once in shared Rust semantic code.

### Rust public API

Rust authoring is first-class, not a wrapper around Python, JavaScript, WASM, JSON, or a transport model.

The idiomatic Rust API calls shared semantic operations directly and must support normal `Scene`, `Mobject`, animation, signal/reactive, query, mutation, lowering, execution, and rendering workflows entirely inside the Rust environment.

A native Rust application must be able to build and render a Noon scene without initializing any language host or serialization subsystem.

### Python may own

- Manim-compatible class hierarchy and method signatures;
- Python argument normalization and iterable/vector conversion;
- Python callable identity and callback invocation;
- Scene subclass discovery;
- Python-appropriate exceptions and wrapper metadata.

### Python must not own

- object ID or painter-order allocation;
- semantic object state;
- scene membership/lifecycle;
- family traversal or ordering semantics;
- layout/bounds calculations;
- target-state evaluation;
- animation timing/scheduling/interpolation;
- signal evaluation;
- transaction rollback state;
- a second retained-text scene;
- a second renderer/runtime model.

The desired shape is a thin proxy API:

```python
class Mobject:
    def shift(self, value):
        self._handle.shift(value)
        return self

class Scene:
    def add(self, *objects):
        self._handle.add([x._handle for x in objects])
```

The Rust public API calls the same semantic operations directly, without any wrapper or serialization hop.

---

## 5. Analysis and lowering

The compiler lowers the Semantic Scene into an Execution Plan.

For native Rust, this is an ordinary typed in-memory Rust transformation. The Semantic Scene is not serialized into a wire document and reparsed by the compiler.

Lowering is allowed to discard authoring structure whenever doing so preserves observable behavior.

The optimizer classifies dependencies/properties, not whole scenes, into these execution classes:

### Immutable

No runtime change is possible.

Examples: static geometry, constant styles, immutable resource topology.

Compile/cache/pack these completely.

### Timeline

The value is predetermined by authored time.

Examples: ordinary transforms, fades, reveals, deterministic composition.

Lower these to explicit tracks, prepared morph/reveal data, or GPU-evaluable parameters.

### Native reactive

The value changes at runtime, but Noon understands the dependency graph.

Examples: `ValueTracker`, pointer position, viewport size, property bindings, built-in constraints and derived expressions.

Evaluate only the dirty dependency closure.

### Host dynamic

Correct behavior requires arbitrary host-language execution.

Examples: an updater containing arbitrary Python control flow or an event handler that calls user Python code.

Represent this explicitly as host callback slots. A few host-dynamic dependencies must not make unrelated static/timeline/reactive content dynamic.

---

## 6. Execution Plan

The Execution Plan is renderer-independent, compact, validated execution data.

It may contain:

- stable execution slots mapped from semantic identities;
- immutable content/resource references;
- resolved timeline channels and event schedules;
- precomputed geometry/morph/reveal plans;
- native reactive bytecode/graphs;
- mutable property slots;
- host callback descriptors;
- event subscriptions;
- invalidation metadata;
- bounds/spatial-index data needed by execution.

It is not required to preserve the Semantic Scene's ergonomic hierarchy.

`noon-core` should converge on this normalized execution-level responsibility. Authoring compatibility helpers do not belong there.

---

## 7. Runtime

A runtime instance owns current execution state:

- playhead and timeline cursors;
- current mutable property values;
- signal/input values;
- dirty/invalidation sets;
- lifecycle/presence state;
- spatial-index state;
- host callback requests/results;
- execution/resource generations;
- renderer-facing change sets.

Static clean content should disappear from ordinary per-frame CPU work.

### Runtime complexity contract

```text
clean paused/static frame     ~ O(0) meaningful CPU work
timeline work                 ~ O(events crossed + active CPU channels)
reactive work                 ~ O(dirty dependency closure)
property edit                 ~ O(affected slots)
structural edit               ~ O(local dependencies + required relowering)
visibility query              ~ O(index query + candidates)
render preparation            ~ O(dirty resident state + visible projection)
GPU upload                    ~ O(changed ranges/resources)
draw submission               ~ O(visible batches/instances)
host bridge                   ~ O(host-relevant state)
resource regeneration         ~ O(actual resource changes)
```

No feature may silently introduce an O(total-scene) fallback for a local operation.

---

## 8. Mutations

All authored/live structural and property changes use one mutation vocabulary and atomic transaction model.

Conceptually:

```text
MutationTransaction
  SetProperty
  SetSignal
  ReplaceContent
  AddNode
  RemoveNode
  AddMember / RemoveMember
  AddAnimation / RemoveAnimation
  ChangeSubscription
```

A transaction is validated before commit.

Each operation has an impact class so the compiler/runtime can perform the minimum required work:

```text
color/translation change  -> property slot update
signal change             -> reactive dirty propagation
path/content change       -> regenerate affected resource
add/remove/reparent       -> local structural update + bounded relowering
large semantic rewrite    -> wider relowering only when genuinely required
```

Host callbacks, editor actions, graph topology updates and hot reload should reuse this machinery rather than inventing separate patch systems.

---

## 9. Host callbacks and interaction

Native interaction is preferred when semantics are known. Arbitrary host behavior remains a supported first-class execution class.

A callback phase is transactional and batched:

```text
runtime frame/input
      |
      v
coherent callback snapshot
      |
      v
host callback phase
      |
      v
one mutation transaction
      |
      v
validate + commit + dirty propagation
      |
      v
render
```

Do not cross Python/WASM once per property getter/setter.

If no host callback slots exist, playback must not require the host interpreter.

Input semantics distinguish:

- sampled state, where latest value wins (pointer position, viewport size);
- discrete ordered events, where occurrences must not collapse (press/release/key events).

Paused scenes may react to input without advancing authored timeline time.

---

## 10. Geometry, text and resources

Semantic content remains high-level where useful; compilation chooses the cheapest retained representation.

Examples:

- simple shapes: analytic/instanced where profitable;
- arbitrary static paths: cached tessellation;
- morphs/reveals: precomputed compatible geometry/arc-length data;
- text: shaped glyph runs and atlas rendering by default;
- text outlines: lazy, only when path-level semantics require them;
- images/meshes: immutable retained resources with localized replacement.

Transform/style/visibility-only changes must not regenerate immutable content.

Text, Graph, 3D and interaction are features of the same scene/runtime architecture, not separate scene engines.

---

## 11. Browser topology

The browser keeps arbitrary Python away from the render frame loop. This section describes a web integration topology only; it is not part of the native Rust execution path.

Target shape:

```text
Python worker / authoring context
  Pyodide
  thin Python facade
  shared semantic Rust/WASM
  arbitrary Python callbacks
          |
          | typed scene/mutation/callback payloads
          v
execution/render context
  execution runtime WASM
  native input processing
  retained renderer
```

Exact worker placement is an integration decision, not a semantic boundary.

A browser worker/process boundary may require a typed transport representation because it is a real cross-context boundary. That transport is derived from authoritative semantic/execution state and must not become another scene model.

JSON may exist for debugging/export/tests or an explicitly justified external boundary. It is not the normal typed authoring API, not an internal Rust layer boundary, and not a per-frame mutation protocol.

---

## 12. Renderer contract

Renderers consume runtime/execution state; they do not own semantic truth.

Required properties:

- retained GPU residency;
- stable resource generations;
- dirty-range uploads;
- painter-order correctness;
- visibility/culling driven by execution-owned bounds/spatial data;
- no retessellation for transform/style-only changes;
- WebGPU and supported fallback backends must agree semantically and visually within reviewed tolerances.

The renderer is usable directly from the native Rust runtime and through web integration. Web integration does not own renderer semantics.

Renderer-specific mirrors and caches are derived and disposable.

---

## 13. Crate and module boundaries

Crates exist only for real dependency, compilation-target or reuse boundaries.

Target responsibilities:

```text
noon
  public Rust API
  authoritative Semantic Scene
  shared authoring semantics

noon-core
  normalized renderer-independent Execution Plan data

noon-compile
  semantic analysis, specialization and lowering

noon-runtime
  mutable execution, scheduling, reactive evaluation, local mutations

noon-render-wgpu
  retained GPU renderer usable by native and web integration

noon-web
  optional browser/WASM integration

supporting crates such as geometry/text
  only where dependency or compilation isolation is genuinely useful
```

The native Rust dependency path must not require `noon-web`, Pyodide, JavaScript, a browser runtime, or a serialization crate merely to move data between engine layers.

Rules:

- `noon-ir` is not a permanent architectural layer. Serialization/transport should become a codec owned by the layer that needs it; delete the crate unless an independent consumer justifies it.
- no crate exists solely for migration compatibility or naming symmetry;
- no `legacy` public module survives the consolidation;
- module structure must reflect ownership directly; do not hide unrelated domains behind `#[path]` or `include!` aggregation modules;
- prefer modules over crates until an actual dependency boundary appears.

---

## 14. Correctness invariants

1. High-level semantics are implemented once in shared Rust.
2. The Semantic Scene is the only authored scene authority.
3. Rust authoring is a first-class direct API; Python/JS frontends contain handles/adapters, not scene engines.
4. The native Rust path `Rust API -> Semantic Scene -> Execution Plan -> Runtime -> Renderer` uses typed in-memory Rust data and requires no serialization, JSON, WASM, browser runtime, or language host.
5. Execution and renderer identities never replace semantic identity.
6. Mutations are atomic.
7. Static regions are not invalidated by unrelated dynamic changes.
8. Reactive evaluation visits only affected dependencies.
9. Host callbacks observe coherent snapshots and commit mutations transactionally.
10. No host interpreter is required when no host-dynamic behavior exists.
11. Direct seek agrees with forward evaluation wherever semantics are deterministic.
12. Offline and realtime rendering use the same semantic/runtime behavior.
13. Unsupported compatibility behavior is explicit; silent approximation is not acceptable.
14. Local changes remain local unless semantics genuinely require wider work.
15. Serialization is used only at explicit external/cross-context boundaries and never dictates the in-memory engine architecture.

---

# Roadmap

The roadmap is deliberately short. Detailed implementation checklists belong in issues/PRs, not permanent competing plan documents.

## Phase A — architecture consolidation

**This phase blocks broad new feature expansion. Correctness fixes may proceed at any time.**

### A1. Make one Semantic Scene authoritative

- turn the existing stable semantic identity/family work into the actual scene authority;
- store semantic content, transform/style, lifecycle, source identity, animation intent and reactive declarations in that scene;
- choose one semantic value model and remove permanent legacy/semantic duplicates;
- make the only architectural boundary `Semantic Scene -> Execution Plan`;
- make that boundary a direct typed in-memory Rust API, not serialization through a scene/wire document.

**Done when:** no normal authoring path requires `SceneDefinition`, `SceneSpec`, retained sidecars or another scene-shaped structure as a second authority, and native Rust lowering requires no serialized intermediate.

### A2. Replace Rust legacy authoring

- move `Scene`, `Mobject`, shapes, layout, `.animate`, lifecycle and composition onto the authoritative Semantic Scene;
- keep the complete Rust authoring -> lowering -> runtime -> renderer path inside Rust with typed in-memory data;
- delete `noon::legacy` and compatibility aliases;
- update internal users directly rather than adding adapters.

**Done when:** the public Rust API has one implementation path, no legacy authoring module, and a native Rust application can author and render without Python, JavaScript, WASM, browser infrastructure, JSON, or serialization bridges.

### A3. Make Python a thin facade

- bind Python `Scene`/`Mobject` wrappers directly to semantic handles;
- delete Python-owned object/track allocation, painter ordering, scheduling, snapshot evaluation and rollback semantics;
- delete retained-text sidecar ownership;
- remove monkey-patched canonical-scene migration code;
- replace JSON bind/update/finalize calls with typed WASM calls.

**Done when:** Python cannot construct a second valid Noon scene without the shared Rust semantic implementation.

### A4. Remove obsolete scene/IR models

- delete legacy/mixed/semantic transport models that only exist for migration;
- remove `from_legacy*` paths and compatibility validators;
- delete `noon-ir` unless a real independent versioned interchange consumer exists;
- keep only explicit debug/export/transport codecs that serialize authoritative data without becoming authority themselves.

**Done when:** repository-wide search finds no migration scene model or production legacy wire path, and the normal native Rust authoring/execution pipeline performs no serialization between engine layers.

### A5. Normalize modules and crates

- reorganize `noon-core` and `noon-runtime` so filesystem/module ownership matches the architecture;
- remove `#[path]`/`include!` structures used to hide unrelated domains;
- split oversized modules by responsibility;
- consolidate text/render/helper crates that lack a real independent dependency boundary.

**Done when:** a contributor can locate semantic, compile, runtime and renderer ownership from the workspace/module tree without knowing migration history.

### A6. Ratchet the architecture

Add structural CI/tests that prevent reintroduction of:

- Python-owned scene/timeline engines;
- legacy scene types in normal authoring;
- serialized JSON/wire intermediates inside the native Rust engine path;
- multiple semantic ID allocators;
- renderer-owned semantic state;
- local operations that fall back to full-scene work.

**Phase A exit:** one semantic scene, one typed in-memory lowering boundary, one runtime, a fully Rust-native authoring/rendering path, thin optional frontends, and no migration architecture.

---

## Phase B — complete common 2D semantics

Build breadth only on the consolidated architecture.

Order:

1. core `Mobject`/`VMobject` family, bounds, layout, state/copy/target and path-query semantics;
2. native `Text`/`MarkupText`, Typst/MathTypst part semantics, then real `Tex`/`MathTex` and numeric text;
3. lines/arrows/tips/dashes and remaining common shapes;
4. transforms, creation/fading/composition and updater animation breadth;
5. coordinate systems, plotting and labels;
6. Graph/DiGraph using shared family/topology/dependency semantics;
7. SVG/images, braces, matrices, tables and moving-camera features.

Every supported Python feature must use shared semantic behavior and add representative ManimCE differential evidence.

**Phase B exit:** representative common 2D Manim scenes require only the intended import/browser adaptation and run through the same semantic/runtime path from Python and Rust.

---

## Phase C — native interaction, locality and live authoring

- finish native pointer/keyboard/viewport ingress;
- lower known updater/constraint behavior to native reactive dependencies;
- finish arbitrary host callback slots and batched callback transactions;
- make retained family/text updates resident and dirty-member-local;
- complete spatial culling and dirty GPU upload locality;
- add editor/session state above semantic identity (selection, hover, drag, undo grouping);
- implement hot reload by reconciling stable source/semantic identities and preserving compatible runtime/resource state.

**Phase C exit:** common interaction works without Python; arbitrary Python callbacks are bounded and explicit; local edits stay local through execution and rendering.

---

## Phase D — 3D and broader capability

Extend the same architecture:

- 3D semantic transforms and camera behavior;
- immutable mesh resources;
- retained depth/mesh rendering on the existing renderer/runtime;
- surfaces, solids, lighting/material semantics;
- mixed fixed-in-frame 2D/3D composition;
- broader Manim namespace coverage and advanced live-authoring UX.

No separate 3D scene engine or canvas/runtime.

---

## 15. Validation strategy

Architecture is enforced with executable evidence, not screenshots alone.

Required categories:

- native Rust authoring -> lowering -> runtime -> renderer smoke tests with no Python/JS/browser/serialization initialization;
- structural checks that the native Rust engine path contains no JSON/wire round-trip between architecture layers;
- Rust/Python semantic parity for equivalent authoring;
- ManimCE v0.21 semantic/raster/timing differential tests for supported APIs;
- direct-seek versus forward-playback tests;
- mutation atomicity/rollback tests;
- dependency-local reactive tests;
- mixed large-static + small-dynamic performance tests;
- host callback batching/isolation tests;
- renderer dirty-range/residency/culling tests;
- WebGPU/WebGL backend equivalence for supported surfaces;
- browser interactive smoke tests;
- structural architecture tests from Phase A6.

Performance regressions should be measured in terms of active, dirty, visible and affected work, not only total FPS.

---

## 16. Decision rule for new work

Before adding an abstraction, crate, scene representation or compatibility layer, answer:

1. Which of the four architecture layers owns this?
2. Is there already an authority for the same state or behavior?
3. Does this create a second scene, scheduler, identity system or renderer authority?
4. Does a local change remain local?
5. Is the new crate/module boundary required by dependency/compilation/reuse, or merely conceptual organization?
6. Can obsolete code be deleted instead of adapted?
7. Does this introduce serialization or a transport representation where a typed in-process Rust boundary should exist?

For this greenfield project, deletion is preferred over compatibility scaffolding.

---

## 17. Non-goals

- preserving historical Noon APIs or internal formats;
- multiple authoring scene models;
- a Python animation/scheduling engine beside Rust;
- requiring Python, JavaScript, WASM, a browser, JSON, or serialization for native Rust authoring/execution/rendering;
- using serialized wire documents as ordinary in-process boundaries between Semantic Scene, Execution Plan, Runtime, and Renderer;
- a renderer-specific semantic scene;
- separate architectures for text, Graph, interaction or 3D;
- serializable wire structures dictating the in-memory semantic design;
- full-scene work as a convenience fallback for local mutation;
- crate proliferation without a real boundary;
- silent approximation of unsupported Manim behavior.