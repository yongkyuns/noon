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

## Continuation update: browser playback and Python control

The first no-Python browser playback path is now implemented:

- `NoonCanvasPlayer` asynchronously creates a WebGPU adapter/device and canvas surface;
- `requestAnimationFrame` timestamps pass through a tested deterministic `PlaybackClock`;
- each frame advances the persistent `ScenePlayer`, incrementally prepares dirty analytic instances, uploads only changed ranges, and presents through `GpuRenderer`;
- canvas backing-store resize preserves world-space aspect ratio and safely skips zero-sized canvases;
- outdated, lost, occluded, timeout, and suboptimal surface states are handled explicitly;
- renderer counters are exposed to JavaScript for structural/performance smoke checks;
- `web/` contains a serialized-scene demo that loops without Python;
- semantic endpoint-based lines now serialize through the IR and render as packed analytic instances without tessellation;
- derivative-scaled shader coverage and one-pixel proxy margins smooth analytic silhouettes and fill/stroke boundaries at the display's native pixel density;
- rectangle strokes use local-distance geometry for uniform thickness, lines use rounded analytic caps, and premultiplied-alpha blending handles translucent style transitions correctly;
- the browser demo applies ordered palette changes transactionally, displays the accepted sequence, and keeps the playhead running;
- CI builds the release wasm package with a pinned `wasm-pack`, checks the demo JavaScript syntax, verifies the generated JavaScript/TypeScript API surface, and compiles the emitted WebAssembly module;
- an optional module worker lazily loads pinned Pyodide, executes editable Python authoring code away from the main thread, and returns correlated versioned `PatchBatch` messages;
- the dependency-free browser `noon` Python module builds semantic style/transform patches and is tested under normal CPython as well as exercised under Pyodide;
- the browser `noon.Scene` API builds complete versioned scene documents with analytic primitives, styles, transforms, stable insertion-order IDs, and declarative position/rotation/opacity tracks;
- the version-4 worker protocol returns one encoded tagged result containing a `SceneDocument` or `PatchBatch`, plus validated authoring identities for complete scenes;
- `ScenePlayer::replace_scene_json` compiles and evaluates replacement state before committing it, preserving the playhead and resetting the new scene's patch sequence only after a successful swap;
- browser scene replacement retains the existing canvas, WebGPU device, renderer, playback clock, and requestAnimationFrame loop, after which ordered live patches continue at sequence zero;
- `scene_player_perf` now measures release-mode initial load, full replacement, and one-object style/transform patches at 1k/10k/100k objects, with a dated local baseline in `docs/performance.md`;
- the first benchmark exposed quadratic `SceneDocument` object validation; bulk scene construction now validates IDs with hash sets while preserving document order, reducing 100k replacement from 7.70 seconds to 106 ms on the baseline machine;
- current one-object patches are 5.6-13.5x faster than replacement in that CPU-only benchmark, though transactional whole-state cloning still puts 100k patch latency around 18-19 ms;
- explicit Python object/track keys are remapped to persistent numeric runtime IDs across reruns, even when Python insertion order changes;
- compatible rerun documents are diffed after worker parsing into minimal `ScenePatch` batches; geometry and draw-order changes use Rust's transactional full-document reconciliation/replacement fallback;
- measuring the initial Rust-only diff path showed it was slower than replacement because it repeated full JSON decoding, so compatible browser reruns now avoid that cost before entering wasm;
- successful style/transform batches now use a preflighted in-place transaction and update only affected runtime fields while reapplying active tracks for that object;
- the value-patch fast path retains all-or-nothing validation and reduced measured 100k style/transform latency from 17-19 ms to 0.12-0.13 ms; structural batches retain the clone fallback;
- renderer-facing changes accumulate until consumed, so static frames repack and upload nothing while animation/value patches update only their affected packed ranges; seeks, loop wraps, and structural edits conservatively invalidate the full frame;
- the 100k renderer benchmark reduced unchanged preparation from a 1.54 ms full rebuild to about 0.000061 ms and exact instance-buffer payload from 8.8 MB/frame to zero; one changed object repacks and uploads one 88-byte record;
- the worker-transfer benchmark identified parsed object-graph cloning and stringify-based equality as the browser rerun bottlenecks; encoded-result transport reduced measured 100k cloning from about 930 ms to 26 ms, while semantic comparisons reduced one-style diffing from 375 ms to 80 ms;
- measured 100k clone-plus-main-thread rerun processing is now about 0.421 seconds instead of 1.38 seconds; the remaining dominant stage is parsing the 25 MiB result JSON, which points to future binary transport or worker-produced deltas;
- the main thread validates the worker protocol and IR envelope before applying each batch transactionally to Rust, while animation and WebGPU presentation continue independently;
- the release wasm package was built with `wasm-pack` and exercised in a real browser, where a circle, rectangle, and rotating line rendered as three analytic draw batches with a clean console.

The next coherent slice should profile real WebGPU command encoding, rasterization, and presentation in a browser. For 100k complete-scene reruns, JSON parsing is now the measured browser-side ceiling; address it later with compact/binary transport or worker-produced deltas rather than more Rust patch tuning. Rendering and normal playback remain in the main-thread Rust/WebGPU module; Python remains an optional worker-side control plane.

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
- consumable object-level dirty tracking across forward evaluation and live patches

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
- analytic instanced lines with semantic endpoints and round caps
- packed reusable instance buffers
- cached packed instances with coalesced dirty ranges and partial `Queue::write_buffer` uploads
- no per-object draw calls for those primitive classes
- structural test for 100k instances
- upload byte/reallocation counters
- repacked/dirty-instance counters and a 1k/10k/100k release benchmark
- draw-call/instance counters
- `Camera2D` world-to-clip uniform (world coordinates remain renderer-independent)
- WGSL shaders with pixel-aware proxy expansion and derivative-based antialiasing
- local-distance rectangle strokes with uniform physical thickness
- premultiplied-alpha fill/stroke coverage and compositing
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
- browser demo applying ordered raw-JSON patch batches while playback continues
- lazy Pyodide worker with a correlated, versioned request/response protocol
- editable Python demo authoring source that emits `PatchBatch` IR
- complete-scene Python authoring with analytic objects and declarative timeline tracks
- tagged worker results for either `SceneDocument` or `PatchBatch`
- transactional full-scene replacement that preserves the playhead and keeps GPU/canvas/clock resources alive
- fresh ordered patch sequencing after successful scene replacement
- main-thread validation of worker protocol and patch envelopes before Rust application
- JavaScript protocol tests and dependency-free CPython authoring API tests
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
- analytic shader silhouettes and stroke transitions use derivative-scaled coverage rather than hard fragment thresholds
- analytic proxy geometry includes a one-device-pixel margin, rectangle strokes remain locally uniform, and line endpoints use capsule distance fields
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
bash scripts/build-web-demo.sh
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
bash scripts/build-web-demo.sh
```

For edits, run narrower package tests/checks continuously, then run the full local gate before pushing a coherent batch.

Do **not** use GitHub Actions as the interactive compiler anymore. Local Mac validation should catch formatting/type/Clippy/test failures first. CI should validate the pushed coherent batch.

## Completed implementation slice: browser playback and Python worker control

The first genuinely visible browser playback path **without Python** is complete:

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
6. Added ordered live patch controls that preserve playback and reject invalid sequences transactionally.
7. Added a release-package build script and CI contract smoke check for the generated JavaScript, TypeScript declarations, and WebAssembly module.
8. Added a lazy Pyodide module worker and small browser Python API that emit ordered semantic patches without owning the render loop, canvas, GPU, or runtime state.
9. Added an editable Python demo, correlated worker protocol, pre-Rust envelope validation, and JavaScript/CPython tests.
10. Replaced hard shader cutoffs with derivative-aware analytic coverage for smooth silhouettes and fill/stroke transitions, validated by shader contracts and a real WebGPU browser.
11. Added pixel-aware proxy expansion, uniform local-distance rectangle strokes, capsule-based round line caps, and premultiplied-alpha compositing for translucent styles.
12. Added complete Python scene authoring and a tagged worker protocol, then wired transactional playhead-preserving scene replacement into the persistent browser runtime without recreating WebGPU resources.
13. Added repeatable scene-operation timing baselines and replaced quadratic IR import validation with order-preserving bulk construction, cutting measured 100k replacement latency by 72.3x.
14. Added explicit stable Python authoring keys and compatible scene-to-patch reconciliation, retaining a measured transactional fallback for unsafe geometry or ordering edits.
15. Added preflighted in-place style/transform transactions with targeted active-track reevaluation, reducing measured 100k one-object patch latency by roughly 150x without weakening atomic rejection.
16. Added consumable runtime dirty tracking, cached incremental frame preparation, and partial GPU buffer writes; unchanged 100k scenes now repack zero instances and upload zero bytes after their initial frame.
17. Benchmarked browser rerun stages, moved the worker protocol to encoded JSON results, and replaced serialization-based diff equality; 100k clone-plus-main processing improved by roughly 3.3x.

Remaining order:

1. Profile real WebGPU command encoding, rasterization, and presentation separately from the quantified runtime/preparation/upload costs.
2. Add a semantic headless browser smoke test only when CI WebGPU support is reliable; do not make browser screenshot equality the semantic test oracle.

## Browser/Python architecture to preserve

Do not put the WebGPU renderer inside the Pyodide/PyO3 Emscripten module.

The implemented split should remain:

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

Continue on `agent/browser-realtime-architecture`. Use the Mac for all local formatting/build/Clippy/test/wasm validation before pushing. Make coherent implementation batches, keep the PR draft, and let GitHub Actions confirm each pushed batch. The immediate goal is stable authoring identity and minimal scene reconciliation; the existing transactional replacement remains the fallback, and the Rust/WebGPU player must continue to run deployed scenes without Python.
