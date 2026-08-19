# Noon implementation plan

This document turns the architecture in `docs/architecture-plan.md` into an executable, CI-gated delivery plan.

## Delivery rule

Every implementation step is a separate commit or a tightly scoped repair commit. A step is complete only when its GitHub Actions gate is green. If CI fails, the next change must fix that step; architectural work does not advance while the branch is red.

CI is part of the implementation, not post-hoc validation.

## Core architectural seam

All work must preserve this boundary:

```text
Authoring frontend
      |
      v
SceneDefinition / ScenePatch
      |
      v
Compiler
      |
      v
CompiledScene
      |
      v
SceneInstance
      |
      v
FrameState / RenderFrame
      |
      v
wgpu
```

The frontend may be Python, Rust, TypeScript, or another language. The frame-critical semantics do not depend on Python.

## CI gates

The default CI gate grows with the implementation. At minimum it must run:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-features`
4. doc tests through the workspace test invocation

As browser crates are introduced, add:

5. `cargo check -p noon-web --target wasm32-unknown-unknown`

As serialization formats stabilize, add deterministic snapshot/round-trip tests. As the renderer stabilizes, add headless structural renderer tests and a small golden-image suite. Performance benchmarks are tracked separately so normal CI remains deterministic and fast.

## Milestone 0 - planning and baseline CI

### Step 0.1 - architecture plan

Status: complete on the architecture branch.

Deliverable:
- `docs/architecture-plan.md`

### Step 0.2 - executable implementation plan

Deliverable:
- this document

### Step 0.3 - establish CI before architectural code changes

Deliverable:
- `.github/workflows/ci.yml`

Acceptance:
- CI runs on pull requests and pushes to the implementation branch;
- format, clippy, and tests are explicit independent checks or explicit steps in a required check;
- the current workspace is green, or any legacy failure is repaired before Step 1 starts.

## Milestone 1 - Nannou-independent semantic core

Goal: prove that Noon can describe and evaluate animation without a renderer, window, Nannou, or Python.

### Step 1.1 - introduce `noon-core`

Create a new crate that contains only semantic/data types.

Initial dependencies should stay deliberately small. Prefer:
- `glam` for vectors/matrices if useful;
- `serde` only when serialization is needed;
- no Nannou;
- no wgpu;
- no windowing library;
- no Python binding dependency.

Initial public concepts:

```text
ObjectId
GeometryId
TrackId
SignalId
Vec2/Transform2D
Color
Rect
Style
GeometryRef
ObjectDefinition
SceneDefinition
```

Acceptance tests:
- IDs are stable and deterministic;
- object creation preserves insertion identity;
- transforms and styles are renderer-independent;
- crate dependency graph contains no Nannou/wgpu dependency.

### Step 1.2 - define timeline IR

Add declarative animation representation rather than frame callbacks.

Initial scope:
- scalar tracks;
- 2D position tracks;
- opacity/color later if needed;
- explicit start time;
- explicit duration;
- easing enum;
- deterministic key/segment ordering.

Acceptance tests:
- normalized timeline representation is deterministic;
- invalid durations are rejected;
- references resolve to existing objects/properties;
- serialized/debug representation is stable enough for structural tests.

### Step 1.3 - compile `SceneDefinition` to `CompiledScene`

Compilation resolves authoring-oriented representation into dense runtime-oriented data.

Compiler responsibilities in this milestone:
- resolve object references;
- assign dense runtime object indices;
- sort tracks;
- classify static vs dynamic properties;
- reject malformed tracks;
- preserve stable public `ObjectId` mapping.

Acceptance tests:
- identical input produces identical compiled output;
- every runtime index is valid;
- every track references a valid object;
- static objects receive no active timeline work.

### Step 1.4 - deterministic `SceneInstance::evaluate(t)`

Implement renderer-free frame evaluation.

Required properties:
- arbitrary seek;
- deterministic evaluation;
- exact endpoint behavior;
- no wall-clock dependency;
- no historical animation scan in the steady-state sequential path;
- a reference arbitrary-seek path is allowed to use binary search.

Acceptance tests:
- expected values at start/mid/end times;
- `evaluate(t)` repeated twice yields identical `FrameState`;
- sequential stepping to `t` agrees with direct seek to `t` within defined floating-point tolerance;
- seeking backwards and forwards is correct;
- long timelines with few active tracks do not evaluate every historical segment.

### Step 1.5 - introduce `ScenePatch`

Live/interpreted authoring depends on a stable patch protocol.

Initial patches:

```text
CreateObject
RemoveObject
SetTransform
SetStyle
AddTrack
ReplaceTrack
RemoveTrack
```

Acceptance tests:
- property changes apply without rebuilding unrelated objects;
- replacing one track preserves other tracks and object identity;
- deleting an object invalidates or rejects dependent patches deterministically;
- patch application followed by direct evaluation produces the same result as compiling the equivalent full definition.

Milestone 1 exit criterion:

> A Nannou-independent Rust core can describe circles/rectangles as semantic geometry references, compile timeline animations, mutate a live scene through patches, and deterministically evaluate frame state at arbitrary times with comprehensive tests.

## Milestone 2 - high-performance native primitive renderer

Goal: establish the performance ceiling before generic vector-path complexity is added.

### Step 2.1 - `noon-render-wgpu`

Implement wgpu renderer boundaries without coupling wgpu types into `noon-core`.

Initial primitives:
- circle;
- rectangle;
- line.

Use analytic/instanced rendering where practical.

Acceptance:
- transforms/styles arrive from `FrameState`;
- static geometry is not rebuilt per frame;
- 1k/10k/100k primitive benchmark scenes exist;
- instance buffer layout has deterministic structural tests;
- no per-object draw call architecture.

### Step 2.2 - renderer performance counters

Expose internal counters suitable for tests/profiling:
- draw calls;
- instance count;
- bytes uploaded;
- geometry cache misses;
- dynamic object count.

These counters make performance regressions testable without relying on timing in normal CI.

## Milestone 3 - browser runtime without Python

Goal: prove the runtime and renderer work in a browser before introducing Pyodide.

Deliverables:
- `noon-web`;
- `wasm32-unknown-unknown` CI check;
- browser demo loading a serialized/embedded compiled scene;
- WebGPU rendering through wgpu.

Invariant:

> A deployed Noon scene does not require Python.

## Milestone 4 - vector geometry compiler

Add:
- renderer-independent `VectorPath`;
- direct Lyon tessellation behind `noon-geometry`;
- cached meshes;
- precomputed path reveal data;
- precomputed morph plans.

Critical invariant:

> Transform/color/opacity animation never retessellates static path geometry.

Morph endpoint and path-reveal properties must be numerically testable without image comparison.

Current static-path slice:

- renderer-independent move, line, quadratic, cubic, and close commands in `VectorPath`;
- versioned JSON and Python authoring round trips;
- direct deterministic Lyon fill/stroke tessellation in `noon-geometry` with finite-output, bounds, malformed-input, and determinism tests;
- exact path/stroke-width mesh caching with instanced transform/style records;
- transform, color, and opacity changes update instance data without tessellation or geometry upload;
- round joins/caps, a fixed curve tolerance, and a 4x-MSAA WebGPU path pass;
- real-browser Rust and Pyodide-authored path rendering with clean WebGPU validation.

Remaining Milestone 4 work:

- precomputed arc-length metadata and a numerically testable path-reveal property;
- precomputed compatible topology and endpoint-exact morph plans;
- cache lifecycle/eviction for long-lived authoring sessions and larger path performance baselines.

## Milestone 5 - text architecture

Add separate representations for:
- `GlyphRun` for normal high-performance text;
- `OutlineText` for path-level text animation/morphing.

Do not make vector outlines the default representation for all text.

## Milestone 6 - native Python frontend

Add PyO3/maturin bindings only after the semantic core/runtime boundaries are stable.

Python objects are lightweight handles to stable IDs. Normal Python calls build `SceneDefinition` data or emit `ScenePatch` messages.

Acceptance:
- Python REPL can create/mutate a running scene;
- Python is absent from normal compiled timeline evaluation;
- bulk arrays cross the boundary without per-element Python calls.

## Milestone 7 - browser Python

Use a Pyodide Web Worker as the Python control plane and `noon-web.wasm`/WebGPU as the realtime data plane.

Initial IPC may be JSON for correctness. Large geometry/data moves to transferable binary buffers later.

Acceptance:
- editing/running Python does not block the render loop;
- compiled playback runs while Python is idle;
- live Python mutation emits patches to the persistent runtime.

Current browser slice:

- Pyodide loads lazily in a module worker only when authoring code is run;
- a dependency-free Python module emits versioned, ordered `PatchBatch` JSON;
- the main thread correlates responses and validates protocol/IR envelopes before Rust application;
- normal animation, WebGPU presentation, and deployed-scene playback remain independent of Python;
- complete Python-authored `SceneDocument` construction and live reconciliation remain follow-up work.

## Milestone 8 - live code reconciliation

Introduce stable authoring identities and scene diffing:

```text
old SceneDefinition
      |
      | diff
      v
new SceneDefinition
      |
      v
ScenePatch[]
```

Goal: code can change mid-run while compatible object/runtime state is preserved.

## Milestone 9 - signals and reactive graph

Introduce renderer-independent signals/expressions before Python tracing.

Initial expression IR should support pure deterministic operations such as arithmetic, trigonometric functions, vector construction, and signal/time inputs.

First backend: Rust reference interpreter.

## Milestone 10 - compiled Python kernels

Add a FastSim-like symbolic tracing frontend:

```text
Python function
    -> symbolic tracer
    -> Kernel IR / SSA
    -> optimization
    -> CPU tape
```

Unsupported Python remains an explicit interpreted fallback rather than silently changing semantics.

## Milestone 11 - WGSL kernel backend and GPU-driven instances

Lower suitable Kernel IR to WGSL so large homogeneous instance sets can update directly on the GPU.

CPU-reference vs WGSL parity tests are mandatory.

## Milestone 12 - editor and ecosystem

Only after runtime interfaces are stable, build richer application/editor layers:
- egui-based native editor if useful;
- web editor with CodeMirror/Monaco;
- timeline/inspector/profiler;
- notebook integration;
- TypeScript control API;
- optional compatibility helpers inspired by Manim/legacy Noon.

## Correctness strategy applied to every milestone

Prefer tests below rasterization whenever possible:

1. frontend/semantic structural tests;
2. compiler invariant tests;
3. timeline/frame-state numerical tests;
4. geometry structural/property tests;
5. CPU/backend differential tests;
6. native/WASM parity tests;
7. renderer structural tests;
8. small controlled golden-image suite only at the top.

Use property tests/fuzzing for transforms, timeline seek patterns, scene patches, serialization round trips, morph endpoints, bounds, and invalid-reference handling.

## Performance strategy applied to every milestone

Normal CI should test structural performance invariants rather than noisy wall-clock thresholds, for example:
- draw-call count does not scale one-for-one with instanced primitives;
- static geometry upload count remains unchanged across transform-only frames;
- active-track evaluation work does not scale with completed historical segments;
- no unexpected frame-path allocations in code paths where zero-allocation is an explicit invariant.

Separate benchmark jobs can track timing trends for:
- 1k/10k/100k analytic primitives;
- 10k transformed static vector paths;
- 1k morphs;
- text-heavy scenes;
- long timelines;
- large GPU-driven instance sets.

## Current execution scope

The current architecture PR begins with Milestone 0 and proceeds through Milestone 1 in CI-gated commits. Milestone 2 starts only after the semantic core/runtime has a stable tested contract.
