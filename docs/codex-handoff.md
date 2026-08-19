# Codex Handoff: Noon Realtime/Browser Redesign

This document is the handoff point for continuing the Noon redesign from a fresh Codex session with a local Mac checkout.

## Repository state

- Repository: `yongkyuns/noon`
- Working branch: `agent/browser-realtime-architecture`
- Draft PR: #7 (`Plan and begin realtime browser architecture`)
- Handoff base commit before this document: `52be2e67b100bc60612948749f1efe7fd15ce12f`
- PR base: `master`
- PR is currently mergeable and intentionally remains draft.
- Most recent completed CI before this handoff: run #84, green.

The architecture/design context is in:

- `docs/architecture-plan.md`
- `docs/implementation-plan.md`

This file is the concise implementation-status and continuation guide.

## Continuation update: visible browser playback

The first no-Python browser playback path is now implemented:

- `NoonCanvasPlayer` asynchronously creates a WebGPU adapter/device and canvas surface;
- `requestAnimationFrame` timestamps pass through a tested deterministic `PlaybackClock`;
- each frame seeks the persistent `ScenePlayer`, prepares analytic batches, uploads them, and presents through `GpuRenderer`;
- canvas backing-store resize preserves world-space aspect ratio and safely skips zero-sized canvases;
- outdated, lost, occluded, timeout, and suboptimal surface states are handled explicitly;
- renderer counters are exposed to JavaScript for structural/performance smoke checks;
- `web/` contains a serialized-scene demo that loops without Python;
- the release wasm package was built with `wasm-pack` and exercised in a real browser, where two objects rendered as two analytic draw batches with a clean console.

The next coherent slice should add the analytic line primitive, then demonstrate ordered live `PatchBatch` mutation against the running browser player. Browser build automation/headless smoke coverage should be added when the CI cost and WebGPU runner support are acceptable.

## Product/architecture goal

Noon is being redesigned as a realtime mathematical animation system with ergonomic Python authoring and a high-performance Rust/WebGPU runtime.

The key architectural seam is:

```text
Authoring frontend (Python / Rust / JS)
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
FrameState
        |
        v
renderer preparation
        |
        v
wgpu / WebGPU
```

Python is intended to be the primary authoring/control language, but it must not be required in the normal frame-critical path. A compiled scene should play without Python. Live Python should mutate a running scene through ordered `ScenePatch` batches.

API compatibility with legacy Noon or Manim is **not** a requirement. User ergonomics and performance are more important.

## What is implemented

### `noon-core`

Renderer-independent semantic model:

- typed stable IDs (`ObjectId`, `GeometryId`, `TrackId`, `SignalId`)
- `Vec2`, `Transform2D`, `Color`, `Style`, bounds/geometry references
- `SceneDefinition`
- declarative timeline definitions
- `ScenePatch`
- stable IDs and validation
- serde support for language-neutral interchange

The semantic core intentionally has no renderer/window dependency.

### `noon-compile`

`SceneDefinition -> CompiledScene` currently provides:

- dense runtime object indices
- deterministic track ordering
- static/dynamic property classification
- incremental application of object/track patches
- dense-index repair after object deletion

### `noon-runtime`

Persistent renderer-independent `SceneInstance`:

- deterministic arbitrary `seek(t)`
- forward cursor-based evaluation
- direct seek vs sequential evaluation parity
- active-track grouping/cursors
- live patch application while preserving current playhead
- evaluation instrumentation proving completed timeline history is not rescanned on every frame

### `noon-ir`

Versioned language-neutral transport layer:

- `SceneDocument`
- ordered `PatchBatch`
- JSON encoding initially, deliberately behind a versioned envelope
- deterministic scene round trips
- unsupported-version rejection
- malformed object/track references rejected through the normal semantic validation path

JSON is intentionally the initial/debuggable protocol. A binary encoding can replace or accompany it later without changing engine semantics.

### `noon-render-wgpu`

Initial renderer boundary and GPU implementation:

- frame preparation separated from wgpu ownership
- analytic instanced circles
- analytic instanced rectangles
- packed reusable instance buffers
- no per-object draw calls for those primitive classes
- structural test for 100k instances
- upload byte/reallocation counters
- draw-call/instance counters
- `Camera2D` world-to-clip uniform (world coordinates remain renderer-independent)
- WGSL shaders
- current wgpu 29 API
- native backend feature profile
- WebGPU-only wasm feature profile
- wgpu noop-backend validation in CI: creates device, buffers, shaders, pipelines, render target, encodes/submits render pass without requiring physical GPU hardware

### `noon-web`

First browser/runtime control slice:

- persistent `ScenePlayer`
- loads a versioned scene JSON document
- compiles it once
- deterministic seek without frontend re-execution
- ordered patch sequence checking
- transactional patch batches: if a later patch fails, the entire batch leaves scene/runtime unchanged
- current playhead is preserved across live patches
- wasm-bindgen wrapper exposing the same runtime semantics to JavaScript
- native behavioral tests plus `wasm32-unknown-unknown` compile in CI

This gives the control path:

```text
Python / JS / editor
        |
        v
SceneDocument JSON
        |
        v
ScenePlayer / CompiledScene / SceneInstance
        ^
        |
ordered PatchBatch JSON
```

The renderer exists separately and is ready to be connected to this browser runtime.

## Correctness/testing model

Do not treat pixel screenshots as the primary definition of correctness.

The intended pyramid is:

1. semantic/IR tests
2. compiler invariants
3. deterministic timeline/frame evaluation
4. geometry structural/numerical tests
5. CPU/GPU/backend parity
6. native/browser parity
7. final renderer golden/perceptual image tests

Important invariants already being tested include:

- stable deterministic IDs
- invalid track/object references rejected
- `seek(t)` deterministic
- direct seek equivalent to sequential evaluation
- forward evaluation does not rescan completed history
- live patch result equivalent to rebuilding/recompiling equivalent definition
- patch batches ordered and transactional
- GPU instance preparation deterministic
- buffers reused rather than reallocated every frame
- WebGPU and browser runtime cross-compile on CI

## CI structure

`.github/workflows/ci.yml` has two layers.

### Fast architecture gate (runs on draft PR updates)

Runs strict checks on the new architecture workspace:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check -p noon-render-wgpu \
  --target wasm32-unknown-unknown \
  --no-default-features --features web
cargo check -p noon-web --target wasm32-unknown-unknown
```

### Full legacy compatibility

The old Nannou engine is deliberately outside the new architecture workspace so normal iteration does not resolve/build the old dependency graph. Legacy compatibility runs when the PR is ready, on push to `master`, or by manual dispatch.

There is one explicitly quarantined legacy Lyon unit test (`path::tests::partial_path`): the legacy test replays text path events whose contours are already closed and then redundantly calls `builder.close()`. The replacement geometry subsystem should receive new strict tests rather than inheriting this test debt.

## New local Mac development workflow

The Mac Codex session should now be the fast pre-push gate. GitHub Actions remains the authoritative second gate.

Clone/check out:

```bash
git clone https://github.com/yongkyuns/noon.git
cd noon
git checkout agent/browser-realtime-architecture
```

Install/confirm Rust targets:

```bash
rustup default stable
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown
```

Run before every push:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check -p noon-render-wgpu \
  --target wasm32-unknown-unknown \
  --no-default-features --features web
cargo check -p noon-web --target wasm32-unknown-unknown
```

For edits, run narrower package tests/checks continuously, then run the full local gate before pushing a coherent batch.

Do **not** use GitHub Actions as the interactive compiler anymore. Local Mac validation should catch formatting/type/Clippy/test failures first. CI should validate the pushed coherent batch.

## Completed implementation slice: first visible browser playback

The next priority is the first genuinely visible browser playback path **without Python**:

```text
ScenePlayer
   |
   v
SceneInstance / FrameState
   |
   v
FramePreparer
   |
   v
GpuRenderer
   |
   v
HTML canvas / WebGPU surface
   |
requestAnimationFrame
```

Implemented in this order:

1. Connected `noon-web` to `noon-render-wgpu` under the wasm WebGPU feature.
2. Added browser canvas/surface initialization and resize handling.
3. Added a playback clock whose time is supplied by `requestAnimationFrame`.
4. Prepared each `FrameState`, uploaded analytic instance batches, and rendered to the canvas.
5. Added a tiny browser demo loading a serialized scene and playing it without Python.

Remaining order:

1. Add analytic line primitives as another instanced batch (semantic endpoints, no tessellation/per-object draw calls).
2. Add browser-side patch demo showing a running animation changing through ordered `PatchBatch` messages.
3. Add browser build/check automation; if practical add a headless browser smoke test, but do not make browser screenshot equality the semantic test oracle.
4. Then integrate a Pyodide worker as the live Python authoring/control plane.

## Browser/Python architecture to preserve

Do not put the WebGPU renderer inside the Pyodide/PyO3 Emscripten module.

Target split should remain:

```text
Browser main thread
  noon-web / wasm32-unknown-unknown
  wgpu -> WebGPU
  ScenePlayer + renderer

Browser worker
  Pyodide / CPython / Emscripten
  Python authoring + arbitrary slower callbacks

Worker -> main thread
  SceneDocument / PatchBatch messages
  transferable buffers later for bulk data
```

A deployed `.noon` scene should not require Pyodide. Pyodide is for live authoring/control.

## Renderer/performance constraints to preserve

- Keep analytic primitives semantic; do not convert circles/rectangles/lines into generic vector paths at creation.
- Large primitive sets should map to packed GPU instance buffers.
- Static geometry should never be re-tessellated/re-uploaded merely because transforms change.
- Normal frame evaluation should allocate little or nothing.
- Frame cost should scale with active work, not accumulated timeline history.
- `CompiledScene` should remain largely immutable; mutable runtime state belongs in `SceneInstance`.
- Do not expose Lyon/wgpu/Nannou types in the public semantic IR.
- Keep normal text as a future glyph-run path; only outline text when vector-level animation requires it.
- Path morph preprocessing should happen once into a future `MorphPlan`; do not reproduce legacy per-frame flatten/resample behavior.

## Frontend/API principles

The eventual Python API should be designed around the new execution model, not around compatibility. Desired properties:

- declarative normal animation so it naturally compiles
- explicit imperative escape hatch for experimentation
- signals/reactive expressions as first-class future concepts
- bulk/instance APIs for large object counts
- arbitrary Python per-frame callback supported but explicitly a slow path
- later FastSim-like symbolic tracing for `@noon.kernel` -> CPU tape -> WGSL

Conceptual Python direction:

```python
scene = Scene()
circle = Circle(radius=0.5)
scene.add(circle)
scene.play(
    circle.position.to((4, 0)),
    circle.color.to(RED),
    duration=2.0,
)
```

and eventually:

```python
theta = Signal(0.0)
circle.position.bind(vec2(cos(theta), sin(theta)))
scene.play(theta.to(TAU), duration=3.0)
```

Do not build the Python bindings until Rust scene/runtime/render semantics needed by those APIs are solid.

## Useful first commands for the new Codex session

```bash
git status -sb
git log --oneline --decorate -20
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Then read:

```text
docs/codex-handoff.md
docs/architecture-plan.md
docs/implementation-plan.md
crates/noon-core/
crates/noon-compile/
crates/noon-runtime/
crates/noon-ir/
crates/noon-render-wgpu/
crates/noon-web/
.github/workflows/ci.yml
```

## Instruction for continuation

Continue on `agent/browser-realtime-architecture`. Use the Mac for all local formatting/build/Clippy/test/wasm validation before pushing. Make coherent implementation batches, keep the PR draft, and let GitHub Actions confirm each pushed batch. The immediate goal is a visible browser WebGPU animation driven by the existing deterministic runtime, with no Python required for playback.
