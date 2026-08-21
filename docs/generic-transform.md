# Generic Transform architecture

## Status

Generic `Transform` is now a first-class semantic animation in the realtime/browser architecture. It is not implemented as a collection of frontend-only position/style tracks and it no longer encodes path morph targets by mutating the source path in authoring IR.

The language-neutral contract is:

```rust
Property::Transform
TrackValues::Object {
    from: ObjectSnapshot,
    to: ObjectSnapshot,
}
```

An `ObjectSnapshot` contains geometry, transform, and style but no scene identity. The source `ObjectId` remains stable for the lifetime of the animation.

The compiler selects an explicit geometry strategy for every Transform:

```rust
TransformGeometryPlan::Static
TransformGeometryPlan::Circle { from_radius, to_radius }
TransformGeometryPlan::Rectangle { from_size, to_size }
TransformGeometryPlan::Line { from_start, from_end, to_start, to_end }
TransformGeometryPlan::PathPair(prepared_geometry)
```

This keeps primitive interpolation analytic, path interpolation precomputed, and unsupported geometry changes explicit.

## Python authoring model

Detached objects are valid Transform targets without becoming rendered scene objects:

```python
source = scene.path(
    source_path,
    fill=None,
    stroke=WHITE,
    stroke_width=0.1,
    stroke_join="round",
    stroke_cap="round",
)

target = Path(
    target_path,
    position=(2.0, -1.0),
    rotation=0.5,
    scale=(1.5, 0.75),
    fill=None,
    stroke=BLUE,
    stroke_width=0.1,
    stroke_join="round",
    stroke_cap="round",
    opacity=0.6,
)

scene.play(
    Transform(source, target),
    duration=2.0,
    easing="ease_in_out_cubic",
)
```

Supported semantic stroke policies are:

- joins: `round`, `miter`, `bevel`;
- caps: `round`, `butt`, `square`.

Round join/cap remain the default, preserving the previous static-Lyon appearance. The same style fields are available on Scene path constructors and style patches. Rust/IR deserialization uses serde defaults for both new fields, so older serialized `Style` payloads that omit them continue to deserialize as round/round.

`Transform(source, VectorPath(...))` remains a convenience form. It snapshots the current/source transform and style and replaces only the target geometry.

Targets are copied when scheduled. Later mutation of a detached target does not change an already-authored Transform.

Sequential Transforms on one Python object chain snapshots: the previous target becomes the next source snapshot. Overlapping generic Transforms for the same object are currently rejected by the Python authoring layer because precedence between two whole-object snapshots would otherwise be ambiguous.

The playground includes both path Transform and analytic primitive Transform examples. The analytic example exercises circle radius, rectangle size, and line endpoint interpolation through the same `scene.play(Transform(...))` API.

## Compiler lowering

The compiler keeps the semantic `from`/`to` snapshots on one compiled Transform track and creates a `TransformGeometryPlan` describing how geometry changes should execute.

### Same geometry

If source and target geometry are identical, the compiler emits `TransformGeometryPlan::Static`. Transform, scale, rotation, fill/stroke color, opacity, and other supported style channels interpolate from the snapshots without a renderer geometry override.

### Analytic primitive -> same analytic primitive

Same-kind analytic geometry is interpolated without path conversion:

- `Circle -> Circle`: radius;
- `Rectangle -> Rectangle`: size;
- `Line -> Line`: start and end points.

The runtime writes these interpolated values directly into semantic `FrameObjectState.geometry`. The analytic renderer consumes the same values through its packed instance records, so native/runtime consumers and WebGPU rendering see the same geometry state.

No Lyon tessellation, path cache entry, path geometry upload, or renderer-only morph geometry is involved.

### Vector path -> vector path

Geometry-changing path Transforms use deterministic path correspondence and a fixed-topology dual-position stroke mesh. Correspondence/tessellation work happens before steady playback. The renderer receives one prepared source/target geometry pair and a normalized morph parameter.

Stroke topology is selected by semantic `StrokeJoin` and `StrokeCap` policy shared with static paths. Static paths lower those policies directly into Lyon. Morph paths use a deterministic fixed-topology segment/join/cap representation:

- every centerline segment has an independent quad;
- bevel and miter joins use fixed fan topology;
- round joins use a fixed eight-segment arc fan;
- round caps use a fixed eight-segment semicircle fan;
- square caps extend the endpoint segment by half the stroke width;
- both left and right join slots exist for every sampled join; the inactive side collapses to the center point.

Keeping both side slots is important: source and target paths may change turn direction without changing vertex/index topology, so the GPU can continue interpolating fixed source/target positions.

Current safety boundary:

- path stroke width must remain constant across a geometry-changing Transform;
- path stroke join and cap policy must remain constant across a geometry-changing Transform because they select mesh topology;
- geometry-changing filled paths are rejected because the current fixed-topology morph mesh is stroke-only;
- unsupported cross-kind geometry Transforms are rejected before runtime.

This is intentional. Noon must not silently fall back to per-frame Lyon tessellation.

## Runtime semantics

`FrameObjectState` remains semantic.

For analytic primitive Transforms, geometry itself is interpolated at every evaluated time: a circle frame contains the actual current radius, a rectangle contains the actual current size, and a line contains the actual current endpoints. `FrameState::render_geometries` remains `None` for these cases.

For path Transforms, semantic geometry remains the exact source endpoint before completion and the exact target endpoint at completion; renderer-prepared fixed-topology morph geometry is stored separately in `FrameState::render_geometries`.

This separation prevents a GPU optimization artifact such as `source.with_morph_target(target)` from leaking into the renderer-independent scene state.

Transform/style interpolation is deterministic under direct seek and forward playback. Narrow property tracks are applied after the generic Transform group, so an explicit `Position`, `Rotation`, or `Opacity` track overrides that corresponding channel while the remaining Transform channels continue normally.

Path Transform progress and path reveal remain independent.

## Performance contract

For an active same-kind analytic Transform:

- geometry remains analytic;
- no path conversion or tessellation occurs;
- no path mesh is inserted into the cache;
- no path geometry buffer is uploaded;
- a changed object repacks and dirties only its analytic instance record.

For an active geometry-changing path Transform:

- correspondence is not recomputed per frame;
- path tessellation is not rerun per frame;
- geometry buffers are not re-uploaded per frame;
- join/cap topology is prepared once with the morph mesh;
- the fixed path mesh is cached;
- the path cache key includes stroke width, join, and cap policy;
- steady frames dirty only the path instance record;
- semantic and renderer path allocations are reused across steady forward frames.

The runtime only clones semantic or prepared path geometry when the selected geometry actually changes, such as entering a different sequential Transform pair or reaching a semantic endpoint. A regression test compares the underlying path command-buffer addresses across successive steady frames so accidental deep cloning becomes a structural test failure.

## Validation

Coverage is split across independent layers:

- Python: detached targets, snapshot-by-value behavior, stable source identity, multiple and sequential Transforms, overlap rejection, old VectorPath convenience syntax, analytic detached targets, and stroke join/cap validation/serialization;
- IR/core: object-snapshot Transform round trips; the new stroke fields are serde-defaulted for backward compatibility;
- compiler: explicit `Static`/`Circle`/`Rectangle`/`Line`/`PathPair` plans, unsupported cross-kind rejection, fill/stroke-width safety boundaries, and rejection of join/cap topology changes during geometry-changing path Transform;
- geometry: direct Lyon reference checks, theoretical cap extents and miter intersections, contour-start/winding invariance, fixed-topology formulas, active-triangle winding, plus static-vs-identity-morph endpoint bounds for all nine join/cap combinations;
- runtime: exact analytic midpoints/endpoints, seek/forward parity, sequential boundary continuity, property precedence, reveal independence, and path allocation stability;
- renderer: join/cap-aware path cache identity, analytic instance-only dirty ranges with zero path work, path no-retessellation/cache-miss behavior, prepared-geometry switches between sequential path pairs, and packed-`PathVertex` static-vs-morph endpoint parity for all nine join/cap combinations;
- browser playground: separate path and analytic primitive Transform examples;
- full CI: format, workspace compile, strict Clippy, both geometry correctness suites, all workspace tests, WebGPU wasm compile, browser-runtime wasm compile, and browser package validation.

## Current limitations / next work

The completed generic Transform and stroke-fidelity milestones support same-kind analytic geometry (`Circle`, `Rectangle`, `Line`) plus stroke-only vector-path geometry changes with shared round/miter/bevel joins and round/butt/square caps.

Remaining limitations include:

- cross-kind geometry interpolation;
- filled path morphing;
- path stroke-width interpolation across geometry-changing morphs;
- changing join/cap topology during an active geometry-changing path Transform;
- `ReplacementTransform`, `TransformFromCopy`, or matching-shape variants.

The next geometry milestone should evaluate filled path Transform with a deliberately bounded first contract: compatible simple closed contours with a stable triangulation, explicit rejection of unsafe/self-crossing cases, and no per-frame tessellation. Richer Transform lifecycle/matching variants can then build on the same semantic `ObjectSnapshot` infrastructure.