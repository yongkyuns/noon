# Noon Architecture Plan

## Status

This document captures the proposed direction for evolving Noon from its current experimental Nannou/Bevy-ECS implementation into a production-oriented, real-time, browser-capable mathematical and illustrative animation engine.

The plan intentionally does **not** require strict compatibility with either the existing Noon API or Manim. User ergonomics and performance are the primary goals.

---

## 1. Product goals

Noon should provide a high-level animation environment with Python-class ergonomics while using a compiled graphics-engine execution model underneath.

Primary goals:

- excellent authoring ergonomics;
- smooth real-time playback and interaction;
- first-class browser deployment;
- deterministic offline rendering using the same execution engine as real-time playback;
- efficient rendering of very large numbers of objects;
- a path to GPU-driven animation and computation;
- low allocation and predictable frame-time behavior;
- testability and correctness that do not depend primarily on visual inspection.

Non-goals:

- strict source compatibility with Manim;
- strict compatibility with the current Noon API;
- reproducing Manim's internal object/update semantics;
- using Python as the frame-critical execution environment.

---

## 2. Core design principle

> **Python should be a primary authoring language, not the execution engine.**

Noon should separate user-facing languages from its runtime representation:

```text
                 Frontends
       +-----------+-----------+
       |           |           |
    Python       Rust      TypeScript
       |           |           |
       +-----------+-----------+
                   |
                   v
             Authoring IR
                   |
               compile()
                   |
                   v
            CompiledScene
                   |
             Runtime evaluator
                   |
          dirty / cull / batch
                   |
                   v
                 wgpu
          +--------+--------+
          |                 |
        native            browser
```

The important contract is the language-neutral scene representation, not any individual frontend API.

---

## 3. Three scene representations

The architecture should explicitly distinguish authoring, compilation, and execution.

### 3.1 `SceneDefinition`

Mutable, ergonomic, high-level representation used by frontends.

Responsibilities:

- object creation;
- hierarchy and grouping;
- animation commands;
- signals and constraints;
- user-visible names and handles;
- references between objects;
- high-level geometry descriptions;
- authoring metadata.

This layer may use richer object structures and can prioritize ergonomics over runtime efficiency.

### 3.2 `CompiledScene`

Immutable or mostly immutable representation produced by the scene compiler.

Compilation should resolve:

- object IDs to dense runtime indices;
- object references;
- animation timing;
- property tracks;
- static versus dynamic state;
- geometry assets;
- path morph correspondence;
- path-reveal metadata;
- text shaping and/or glyph outlines;
- instance batches;
- culling metadata;
- reactive expression graphs;
- compiled CPU/GPU kernels where possible.

The compiled representation should be serializable and portable where practical.

A compiled `.noon` artifact could eventually contain:

```text
scene.noon
+-- object table
+-- hierarchy
+-- property tracks
+-- geometry assets
+-- morph plans
+-- text/glyph assets
+-- signals
+-- kernels
+-- textures
+-- metadata
```

A browser should be able to play a compiled Noon scene without loading Python.

### 3.3 `SceneInstance`

Mutable runtime state associated with one execution of a `CompiledScene`.

Typical state:

- current time;
- signal/parameter values;
- active-track cursors;
- current transforms;
- current styles;
- current bounds;
- dirty bitsets;
- culling state;
- GPU buffer offsets;
- interaction state;
- event state.

The same `CompiledScene` should support multiple independent `SceneInstance`s.

---

## 4. API philosophy

Noon should be declarative by default, while preserving imperative escape hatches.

Example canonical style:

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

Syntactic sugar can provide a more fluent form:

```python
scene.play(circle.animate.move_to(4, 0))
```

Both should lower to the same timeline representation.

The API should not reproduce Manim semantics merely for compatibility. Familiar vocabulary may be retained when it improves usability, but Noon should use semantics appropriate for a compiled runtime.

---

## 5. Signals and reactive animation

Per-frame Python callbacks should not be the primary reactive-animation model.

Prefer explicit signals and dependency graphs:

```python
theta = Signal(0.0)

circle.position.bind(
    vec2(cos(theta), sin(theta))
)

scene.play(theta.to(TAU), duration=3.0)
```

This exposes a dependency graph such as:

```text
       theta
       /   \
    cos     sin
     |       |
     x       y
      \     /
      position
```

That graph can be:

- evaluated by a compact Rust runtime;
- optimized;
- vectorized;
- lowered to WGSL;
- executed for many instances on the GPU.

Arbitrary Python callbacks may remain as a fallback, but they should be clearly classified as an interpreted slow path.

---

## 6. Compiled Python kernels

Noon should eventually support FastSim-like tracing for numerical animation functions.

Example:

```python
@noon.kernel
def orbit(t, radius, omega):
    angle = omega * t
    return vec2(
        radius * cos(angle),
        radius * sin(angle),
    )
```

Conceptual lowering:

```text
Python function
      |
      v
symbolic tracer
      |
      v
expression / SSA IR
      |
  optimization
      |
      +----------+
      |          |
      v          v
 CPU tape      WGSL
```

The initial implementation should favor a simple expression IR and efficient Rust evaluator. WGSL lowering can follow after semantics are stable.

Unsupported Python should fall back explicitly rather than silently changing semantics.

---

## 7. Bulk and instanced objects

High object counts must be first-class in the API.

Do not require one Python object and one runtime entity per visual primitive for large datasets.

Example:

```python
particles = CircleInstances(
    positions=xy,
    radius=0.01,
    colors=rgba,
)
```

or:

```python
particles = scene.instances(
    Circle(radius=0.01),
    positions=xy,
    colors=rgba,
)
```

This should map naturally to packed GPU instance buffers.

The runtime should distinguish between:

- individually addressable objects;
- homogeneous instance collections;
- arbitrary vector paths;
- text/glyph runs;
- GPU-driven particle/field data.

---

## 8. Runtime representation

Bevy ECS is useful during experimentation and may remain useful in authoring/editor layers, but the compiled runtime should be evaluated against a denser representation.

A possible packed runtime layout:

```text
Transforms
    x[]
    y[]
    scale_x[]
    scale_y[]
    rotation[]

Styles
    fill[]
    stroke[]
    stroke_width[]
    opacity[]

Geometry
    geometry_id[]
    geometry_kind[]

Bounds
    min_x[]
    min_y[]
    max_x[]
    max_y[]

Tracks
    active_track_index[]
```

Advantages:

- cache-friendly iteration;
- straightforward SIMD opportunities;
- compact WASM memory layout;
- efficient GPU uploads;
- predictable ownership;
- easy serialization;
- low runtime overhead.

Stable `ObjectId`s can map to dense runtime indices during compilation.

---

## 9. Timeline representation

The current model of keeping vectors of historical animations on each component should be replaced in the compiled runtime with sorted property tracks.

Example:

```text
Position.x
  [0.0, 1.0]  0 -> 4
  [4.0, 5.5]  4 -> -2
  [8.0, 9.0] -2 -> 1
```

During real-time sequential playback, each track can maintain a cursor.

During random-access seeking or timeline scrubbing, the runtime can binary-search the appropriate segment.

Frame cost should scale with active/dirty state rather than total scene history.

---

## 10. Geometry architecture

Geometry should retain semantic information as long as possible rather than converting all shapes immediately into generic vector paths.

Recommended representation:

| Geometry | Compiled representation | Typical per-frame work |
|---|---|---|
| circle | analytic primitive | instance parameters only |
| rectangle / rounded rectangle | analytic primitive | instance parameters only |
| line | analytic primitive | endpoints/style |
| static arbitrary path | cached tessellated mesh | transform only |
| ordinary text | shaped glyph run / atlas | transform only |
| outline text | cached vector outlines | transform/morph |
| path morph | precomputed compatible topology | interpolate vertices |
| path reveal | cached arc-length metadata | reveal parameter |

### 10.1 Analytic primitives

Circles, rectangles, rounded rectangles, and similar primitives should use analytic/SDF-style shaders where appropriate.

Large homogeneous collections should render using instancing rather than separate path tessellation and draw calls.

### 10.2 Arbitrary vector paths

Use Lyon directly as a geometry/tessellation dependency rather than routing through Nannou.

Static path geometry should be tessellated once and cached.

Transform/color/opacity animation should not cause re-tessellation.

### 10.3 Path morphing

Expensive correspondence work should happen during scene compilation.

Compilation should:

1. flatten/resample source path;
2. flatten/resample destination path;
3. establish compatible topology/correspondence;
4. cache source and destination vertices;
5. emit a `MorphPlan`.

Runtime work becomes approximately:

```text
p[i] = src[i] * (1 - t) + dst[i] * t
```

This can later move into a vertex or compute shader.

### 10.4 Path reveal

`Create`/`ShowCreation`-style effects should use cached arc-length metadata rather than rebuilding partial paths each frame.

---

## 11. Text architecture

Separate normal text rendering from outline/path text.

```text
Text
+-- GlyphRun
|   +-- shaping/layout
|   +-- glyph atlas
|   +-- fast regular labels
|
+-- OutlineText
    +-- vector glyph outlines
    +-- path animation
    +-- morphing
```

Potential dependencies:

- `cosmic-text` for shaping/layout;
- `glyphon` for wgpu glyph rendering;
- `skrifa`/Fontations or similar for glyph outlines.

Converting all text into vector paths should not be the default.

---

## 12. Rendering architecture

Use `wgpu` directly as the production renderer foundation.

Recommended initial stack:

```text
wgpu
 + analytic primitive shaders
 + Lyon tessellation
 + custom batching/instancing
```

`egui` should be optional editor/application UI, not the rendering abstraction.

Possible editor structure:

```text
egui / web UI
+-- object inspector
+-- timeline
+-- playback controls
+-- profiler
+-- viewport
    +-- Noon custom wgpu renderer
```

Keep the renderer behind a clear internal interface so alternative vector renderers can be evaluated later.

---

## 13. Native and browser Python

### Native

Use PyO3/maturin for the Python frontend while keeping frame-critical data and execution in Rust.

### Browser

Do not require Python and wgpu to live inside the same WASM module.

Recommended browser architecture:

```text
Browser
|
+-- render/runtime context
|     +-- noon-web.wasm
|     +-- wasm-bindgen
|     +-- wgpu/WebGPU
|     +-- CompiledScene runtime
|
+-- Python worker
      +-- Pyodide
      +-- CPython
      +-- Noon Python API
```

Communication should use scene definitions, scene patches, events, and transferable binary buffers.

Python should not own:

- frame scheduling;
- GPU resources;
- the canvas;
- high-frequency pointer sampling;
- the normal animation loop.

Compiled scenes should play in the browser without Pyodide.

---

## 14. Interaction model

High-frequency interactions should remain native when possible.

Examples:

- drag constraints;
- pan/zoom;
- hover state;
- hit testing;
- pointer following;
- camera movement.

Python should receive semantic events rather than every raw pointer sample where possible.

Example:

```text
pointer event
    |
Rust hit test
    |
Click(ObjectId)
    |
Python callback
    |
scene patch
```

This preserves interactive flexibility without putting Python in the 60/120 Hz critical path.

---

## 15. Parallelism strategy

Rayon can be useful for native preprocessing and offline/batch work, including:

- path preprocessing;
- morph-plan generation;
- text outlining/shaping batches;
- asset import;
- offline frame preparation;
- CPU-heavy scene compilation.

Browser CPU threading should not be a foundational performance assumption.

Optimization priority:

1. minimize per-frame work;
2. cache static state;
3. batch and instance;
4. use compact data-oriented runtime structures;
5. move suitable work to GPU shaders/compute;
6. add CPU parallelism where profiling shows value.

---

## 16. Correctness and validation strategy

Visual output must **not** imply visual-only testing.

Noon should make each pipeline stage deterministic and inspectable so most correctness can be verified before rasterization.

The test stack should be layered as follows.

### 16.1 Frontend -> `SceneDefinition`

Verify frontend semantics with structural snapshots.

Example input:

```python
scene.play(
    circle.position.to((4, 0)),
    duration=2,
)
```

Expected structural output might be:

```text
object: circle
property: position
start: [0, 0]
end: [4, 0]
t0: 0
duration: 2
ease: default
```

Tests should compare normalized serialized IR rather than images.

This catches API/compiler regressions before geometry or rendering is involved.

### 16.2 `SceneDefinition` -> `CompiledScene`

Compiler tests should verify invariants such as:

- every object reference resolves;
- dense indices are valid;
- tracks are sorted and non-corrupt;
- static objects do not receive dynamic work unnecessarily;
- instance ranges do not overlap incorrectly;
- all geometry references resolve;
- all buffers have valid lengths/offsets;
- no dependency graph cycles exist unless explicitly supported;
- compiled output is deterministic for identical input.

Compiled-scene snapshots can provide strong regression coverage.

### 16.3 Timeline evaluator tests

This should be one of the strongest test layers.

Given a compiled scene and exact time `t`, evaluate a frame state without rendering.

For example:

```text
t = 0.0 -> position = (0, 0)
t = 1.0 -> position = (2, 0)
t = 2.0 -> position = (4, 0)
```

Test:

- easing functions;
- chained tracks;
- overlapping tracks;
- relative operations;
- hierarchy propagation;
- signal dependencies;
- seeking backward/forward;
- random-access versus sequential evaluation;
- boundary conditions at exact start/end times.

A key invariant should be:

> Evaluating the scene directly at time `t` must produce the same semantic frame state as advancing sequentially to `t`.

This makes timeline scrubbing and offline rendering testable without pixels.

### 16.4 Geometry tests

Geometry compilation should expose numerical and structural invariants.

For analytic primitives:

- expected bounds;
- expected signed-distance behavior at selected points;
- transform correctness;
- stroke/fill parameters.

For tessellated paths:

- indices stay within vertex bounds;
- output contains no NaN/Inf;
- winding/orientation is valid where required;
- bounds conservatively contain generated vertices;
- tessellation is deterministic;
- empty/degenerate input is handled intentionally.

For morph plans:

- `morph(t=0)` reproduces the source;
- `morph(t=1)` reproduces the destination;
- vertex counts/topology are compatible;
- intermediate frames contain no invalid values;
- closed paths remain closed when required;
- random-access evaluation matches sequential evaluation.

For path reveal:

- reveal 0 shows none;
- reveal 1 shows all;
- revealed arc length is monotonic;
- no segment is skipped or duplicated unexpectedly.

### 16.5 Text tests

Keep text tests below the pixel layer where possible.

For a fixed font asset and input string, verify:

- glyph IDs;
- glyph positions;
- advances;
- line breaks;
- shaping direction;
- bounding boxes;
- outline extraction where applicable.

Fonts used by tests should be pinned test assets rather than relying on host-system fonts.

### 16.6 CPU/GPU parity tests

Where computation can run on both CPU and GPU, keep a simple reference evaluator.

For example, a compiled kernel should be evaluable by:

```text
reference Rust evaluator
optimized Rust evaluator
WGSL/WebGPU evaluator
```

Generate random inputs and compare outputs within explicit numerical tolerances.

The same approach should apply to:

- animation kernels;
- transform propagation;
- morph interpolation where GPU accelerated;
- particle updates;
- culling predicates where practical.

GPU acceleration should therefore be an optimization of already-defined semantics rather than the only implementation of those semantics.

### 16.7 Native/browser parity

The same serialized `CompiledScene` should be evaluable on native and browser runtimes.

At fixed times, compare semantic `FrameState` data such as:

- transforms;
- colors;
- opacities;
- active geometry IDs;
- instance data;
- bounds;
- draw-list structure.

This provides much stronger browser correctness guarantees than comparing screenshots alone.

### 16.8 Interaction replay tests

Input should be recordable as semantic event sequences:

```text
PointerDown(x, y)
PointerMove(x, y)
PointerUp(x, y)
KeyDown(...)
```

Given a fixed initial scene and event stream, the resulting scene state should be deterministic.

This enables repeatable tests for:

- dragging;
- hover transitions;
- hit testing;
- selection;
- camera controls;
- user-triggered animations.

### 16.9 Property-based tests and fuzzing

Many graphics bugs arise in combinations that hand-written examples miss.

Randomized tests should generate:

- transforms;
- animation sequences;
- path shapes;
- nested groups;
- morph targets;
- extreme scales;
- degenerate geometry;
- seek patterns;
- large instance counts.

Useful invariants include:

- no panics;
- no NaN/Inf propagation;
- deterministic output;
- valid buffer indices;
- conservative bounds;
- identical direct/sequential evaluation;
- exact endpoint behavior for animations;
- stable serialization round-trips.

Fuzzing should target parsers, path processing, compilation, and scene deserialization especially aggressively.

### 16.10 Golden image tests

Golden images should be used at the **renderer integration boundary**, not as the primary correctness mechanism.

Maintain a small canonical scene suite covering:

- primitive fill/stroke;
- clipping/masking;
- transparency;
- gradients;
- text;
- path tessellation;
- morphing;
- camera transforms;
- antialiasing;
- layering/depth;
- instance rendering.

Render scenes at fixed:

- dimensions;
- device pixel ratio;
- time;
- random seed;
- font assets;
- color space/configuration;
- antialiasing settings.

On a controlled CI rendering environment, exact or near-exact image diffs can catch regressions.

Across different GPUs/browsers, allow a documented tolerance or perceptual metric because rasterization/antialiasing may differ slightly.

Do not weaken semantic tests merely to accommodate image variation.

### 16.11 Visual regression gallery

CI should also generate a human-reviewable gallery for canonical scenes.

For each changed renderer/compiler PR, preserve artifacts such as:

```text
reference image
new image
difference image
error metric
```

This is valuable for changes that are intentionally visual but difficult to characterize with one scalar threshold.

### 16.12 Debug/introspection mode

Noon should make runtime state inspectable.

A debug frame dump could include:

```text
FrameDump
+-- time
+-- evaluated object properties
+-- dirty objects
+-- visible objects
+-- bounds
+-- draw batches
+-- geometry IDs
+-- instance ranges
+-- GPU upload ranges
```

This is useful for both automated testing and diagnosis when an image is wrong.

### 16.13 Determinism requirements

Determinism should be treated as an architectural feature.

Prefer:

- explicit scene time instead of wall-clock dependence;
- seeded/stateless randomness;
- pinned test fonts/assets;
- deterministic scene compilation;
- stable ordering rules;
- deterministic serialization;
- explicit viewport/DPI/color-space state.

This directly improves:

- reproducible videos;
- browser/native parity;
- timeline seeking;
- regression testing;
- debugging.

### 16.14 Performance regression tests

Performance is part of correctness for a real-time engine.

Maintain benchmark scenes for at least:

- 1k / 10k / 100k analytic primitives;
- large instance clouds;
- 1k+ simultaneous transforms;
- many static arbitrary paths;
- many path morphs;
- heavy text scenes;
- repeated random-access seeking;
- scene compilation time;
- GPU upload volume;
- allocations per frame.

Track metrics such as:

- median and tail frame time;
- CPU frame evaluation time;
- GPU frame time;
- allocations/frame;
- bytes uploaded/frame;
- draw-call count;
- compile time;
- peak memory.

A visually correct change that causes a 10x frame-time regression should fail review just like a numerical correctness regression.

---

## 17. Testing pyramid

The intended test distribution should be roughly:

```text
                 few
          +----------------+
          | visual goldens |
          +----------------+
          | backend parity |
          +----------------+
          | geometry tests |
          +----------------+
          | frame evaluator|
          +----------------+
          | IR/compiler    |
          +----------------+
          | unit/property  |
          +----------------+
                many
```

The lower layers should catch most problems before a renderer is involved.

---

## 18. Proposed crate organization

A possible future workspace:

```text
noon-core
    object IDs, scene definition, user-facing semantic types

noon-ir
    portable authoring/compiled representations

noon-geometry
    paths, tessellation, morph plans, geometry compilation

noon-compile
    SceneDefinition -> CompiledScene

noon-runtime
    packed timeline/frame evaluator

noon-render-wgpu
    rendering, batching, GPU resource management

noon-text
    shaping, glyphs, optional outline extraction

noon-python
    PyO3 Python frontend

noon-web
    wasm-bindgen browser runtime

noon-ui-egui
    optional native/web editor UI

noon-parallel
    optional native parallel preprocessing
```

This is a conceptual target, not a requirement to split crates immediately.

---

## 19. Dependency direction

Initial likely dependencies:

| Layer | Candidates |
|---|---|
| math | `glam` |
| small containers | `smallvec` |
| serialization | `serde` |
| vector geometry | direct Lyon crates |
| GPU | `wgpu`, `bytemuck` |
| text | `cosmic-text`, `glyphon`, optional `skrifa` |
| native Python | `pyo3`, maturin |
| web | `wasm-bindgen`, `web-sys` |
| optional UI | `egui`, `eframe` |
| optional native parallelism | `rayon` |

Nannou should be removed from the core architecture.

The full Bevy engine should not be required. `bevy_ecs` can be retained temporarily or in authoring/editor layers if it remains useful, while the compiled runtime is evaluated independently.

---

## 20. Implementation phases

### Phase 0 - establish benchmarks and correctness harness

Before major architectural work:

- select canonical visual scenes;
- add deterministic scene-time handling;
- build initial frame-state snapshots;
- establish basic benchmark workloads;
- measure current allocations/frame and frame cost.

This gives the rewrite measurable targets.

### Phase 1 - extract renderer-independent core

- remove Nannou types from public/core scene semantics;
- introduce stable object IDs;
- introduce renderer-independent transforms/colors/geometry descriptors;
- define `SceneDefinition`;
- define a minimal `CompiledScene`;
- preserve a small subset of existing functionality end to end.

### Phase 2 - wgpu renderer

Implement native rendering first for:

- circles;
- rectangles;
- lines;
- static arbitrary vector paths;
- transforms;
- fill/stroke/opacity;
- depth/order.

Then bring the same runtime to browser WebGPU.

### Phase 3 - compiled timeline runtime

- sorted property tracks;
- track cursors;
- random-access seeking;
- dirty flags;
- static/dynamic classification;
- packed frame state.

### Phase 4 - geometry compiler

- cached tessellation;
- path reveal metadata;
- precomputed morph plans;
- text/glyph caching;
- batching/instancing improvements.

### Phase 5 - Python frontend

Build PyO3 bindings around the now-stable semantic model.

The Python API should be optimized for ergonomics rather than old Noon or Manim compatibility.

### Phase 6 - browser authoring

- Pyodide worker;
- scene-definition/patch protocol;
- transferable binary arrays;
- live reload;
- error reporting;
- browser editor integration.

### Phase 7 - reactive expression compiler

- signals;
- expression graph;
- symbolic Python tracer;
- Rust tape evaluator;
- optimization passes;
- CPU/reference parity testing.

### Phase 8 - GPU compute lowering

- WGSL lowering for selected expression kernels;
- GPU-driven instance updates;
- CPU/GPU parity harness;
- large particle/field demonstrations.

### Phase 9 - production features

Examples:

- gradients;
- clipping/masks;
- richer text/math support;
- image/video assets;
- camera systems;
- editor timeline;
- profiling UI;
- export/render pipeline;
- plugin/extension interfaces.

---

## 21. Initial benchmark scenes

The architecture should be evaluated continuously against representative workloads.

Suggested baseline suite:

1. **100,000 moving circles**
   - analytic primitive;
   - per-instance transform/color;
   - validates batching and GPU instance updates.

2. **10,000 static arbitrary paths with animated transforms**
   - validates cached tessellation and transform-only updates.

3. **1,000 simultaneous path morphs**
   - validates morph-plan compilation and runtime interpolation.

4. **Large text scene**
   - many labels plus a subset converted to outline text.

5. **Particle field driven by one compiled kernel**
   - validates bulk data model and future GPU compute path.

6. **Long timeline with sparse active animation**
   - validates that runtime cost depends on active tracks rather than historical track count.

7. **Random-access seeking stress test**
   - validates deterministic direct evaluation.

These targets should be refined based on actual profiling and realistic user workloads.

---

## 22. Architecture invariants

The following should guide implementation decisions:

1. Python is not required in the normal frame-critical path.
2. Static geometry is not regenerated every frame.
3. Transform/color/opacity animation does not trigger tessellation.
4. Runtime work scales primarily with active/visible/dirty state.
5. Large homogeneous object sets can use packed/instanced representations.
6. Random-access evaluation at time `t` matches sequential playback at time `t`.
7. Real-time and offline rendering use the same scene evaluator.
8. Compiled scenes can execute without their authoring frontend.
9. GPU implementations have testable reference semantics where practical.
10. Most correctness is verified numerically/structurally before image comparison.
11. Steady-state frame evaluation should avoid unnecessary heap allocation.
12. Performance regressions are treated as correctness regressions for key benchmark scenes.

---

## 23. Near-term decision order

Before extensive implementation, resolve these in order:

1. exact `SceneDefinition` model;
2. exact `CompiledScene` model;
3. stable object/geometry IDs and ownership;
4. packed runtime/frame-state layout;
5. property-track semantics;
6. renderer interface and first analytic primitive pipeline;
7. geometry caching and morph representation;
8. signal/reactive semantics;
9. Python frontend API;
10. browser authoring protocol;
11. expression/kernel compiler.

The first four decisions are especially important because they determine whether later performance features remain natural or require another rewrite.
