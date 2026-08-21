# Generic Transform architecture

## Status

Generic `Transform` is a first-class semantic animation in the realtime/browser architecture. It is not implemented as a collection of frontend-only position/style tracks and it does not encode morph targets by mutating the source path in authoring IR.

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
    fill=BLUE,
    stroke=WHITE,
    stroke_width=0.1,
)

target = Path(
    target_path,
    position=(2.0, -1.0),
    rotation=0.5,
    scale=(1.5, 0.75),
    fill=PURPLE,
    stroke=WHITE,
    stroke_width=0.1,
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

Round join/cap remain the default. Rust/IR deserialization uses serde defaults for both fields, so older serialized `Style` payloads that omit them remain compatible.

`Transform(source, VectorPath(...))` remains a convenience form. It snapshots the current/source transform and style and replaces only the target geometry.

Targets are copied when scheduled. Later mutation of a detached target does not change an already-authored Transform. Sequential Transforms on one Python object chain snapshots: the previous target becomes the next source snapshot. Overlapping generic Transforms for the same object are currently rejected because precedence between two whole-object snapshots would otherwise be ambiguous.

The playground includes stroke path, filled path, and analytic primitive Transform examples.

## Compiler lowering

The compiler keeps the semantic `from`/`to` snapshots on one compiled Transform track and creates a `TransformGeometryPlan` describing how geometry changes should execute.

### Same geometry

If source and target geometry are identical, the compiler emits `TransformGeometryPlan::Static`. Transform, scale, rotation, fill/stroke color, opacity, and other supported style channels interpolate from the snapshots without a renderer geometry override.

### Analytic primitive -> same analytic primitive

Same-kind analytic geometry is interpolated without path conversion:

- `Circle -> Circle`: radius;
- `Rectangle -> Rectangle`: size;
- `Line -> Line`: start and end points.

The runtime writes these values directly into semantic `FrameObjectState.geometry`. The analytic renderer consumes the same values through packed instance records, so runtime/native and WebGPU consumers see the same geometry state. No Lyon tessellation, path cache entry, path geometry upload, or renderer-only morph geometry is involved.

### Vector path -> vector path

Geometry-changing path Transforms use deterministic feature-preserving correspondence and fixed source/target GPU geometry. Correspondence and topology work happen before steady playback. The renderer receives one prepared path pair and a normalized morph parameter.

Stroke topology is selected by semantic `StrokeJoin` and `StrokeCap`. Static paths lower those policies directly into Lyon. Morph strokes use deterministic fixed topology:

- every centerline segment has an independent quad;
- bevel and miter joins use fixed fan topology;
- round joins use a fixed eight-segment arc fan;
- round caps use a fixed eight-segment semicircle fan;
- square caps extend the endpoint segment by half the stroke width;
- both left and right join slots exist for every sampled join, with the inactive side collapsed.

This allows source and target paths to change turn direction without changing vertex/index topology.

## Bounded filled-path Transform

Filled path Transform is supported for a deliberately bounded class rather than falling back to per-frame triangulation.

The first contract requires:

1. source and target each contain exactly one closed contour;
2. the resampled endpoint contours are simple and nondegenerate;
3. source and target both have fill enabled, or both have fill disabled;
4. stroke width, join, and cap topology remain unchanged across a geometry-changing path Transform;
5. a single center-fan triangulation must remain valid for the complete interpolation interval.

For a filled path, the planner:

1. reuses the normal deterministic, feature-preserving path correspondence;
2. canonicalizes sampled boundary winding to CCW and aligns target cyclically without changing winding;
3. computes each endpoint polygon's area centroid;
4. creates one fan topology from the moving centroid to every adjacent boundary pair;
5. proves every fan triangle retains strictly positive signed area for all `t` in `[0, 1]`.

The last check is continuous rather than frame-sampled. For a triangle whose three vertices move linearly, signed area is a quadratic function of time. The planner evaluates both endpoints and the exact interior critical point when one exists. If the minimum area is not safely positive, compilation rejects the Transform with `UnsafeFilledPathTransform`.

This currently supports useful concave/star-shaped cases such as a rounded loop morphing into a concave star, while intentionally rejecting shapes that cannot be certified by this center-fan contract. It is not a general arbitrary-polygon triangulator.

Fill vertices and stroke vertices live in the same cached prepared path mesh and use the same morph progress. Fill colors still interpolate through the normal semantic style channel. Fill presence itself cannot change during a geometry-changing path Transform because that changes mesh topology/cache identity.

No fill correspondence, triangulation, or tessellation runs per frame.

## Explicit safety boundaries

The compiler rejects rather than silently degrading when a Transform leaves the supported fixed-topology contract:

- fill presence changes during geometry-changing path Transform;
- unsafe/self-intersecting/non-certifiable filled path geometry;
- multiple or open contours for a filled geometry-changing path Transform;
- path stroke-width changes during geometry-changing Transform;
- join/cap topology changes during geometry-changing Transform;
- unsupported cross-kind geometry Transform.

The old stroke-only path behavior is preserved by explicitly treating stroke-only objects as `fill=None`; an open path with fill enabled is a different semantic request and is not accepted by the bounded fill planner.

## Runtime semantics

`FrameObjectState` remains semantic.

For analytic primitive Transforms, geometry itself is interpolated at every evaluated time. `FrameState::render_geometries` remains `None` for these cases.

For path Transforms, semantic geometry remains the exact source endpoint before completion and the exact target endpoint at completion. Renderer-prepared fixed-topology source/target geometry is stored separately in `FrameState::render_geometries`.

This prevents GPU preparation details such as `source.with_morph_target(target)` from leaking into renderer-independent scene state.

Transform/style interpolation is deterministic under direct seek and forward playback. Narrow property tracks are applied after the generic Transform group, so an explicit `Position`, `Rotation`, or `Opacity` track overrides that corresponding channel while the remaining Transform channels continue normally.

Path Transform progress and path reveal remain independent. Filled path runtime coverage also verifies direct-seek/forward parity, interpolated fill/style/transform state, exact semantic endpoints, and the detached prepared render pair.

## Performance contract

For an active same-kind analytic Transform:

- geometry remains analytic;
- no path conversion or tessellation occurs;
- no path mesh is inserted into the cache;
- no path geometry buffer is uploaded;
- a changed object dirties only its analytic instance record.

For an active geometry-changing path Transform, including supported filled paths:

- correspondence is not recomputed per frame;
- stroke topology and fill triangulation are not recomputed per frame;
- path tessellation is not rerun per frame;
- geometry buffers are not re-uploaded per frame;
- the fixed source/target path mesh is cached;
- the path cache key includes path geometry, stroke width, join, cap, and fill presence;
- steady frames dirty only the path instance record;
- semantic and renderer path allocations are reused across steady forward frames.

The runtime only clones semantic or prepared path geometry when the selected geometry actually changes, such as entering a different sequential Transform pair or reaching a semantic endpoint.

## Validation

Coverage is intentionally split across independent layers:

- Python: detached targets, snapshot-by-value behavior, stable source identity, multiple/sequential Transforms, overlap rejection, VectorPath convenience syntax, analytic targets, stroke join/cap serialization, and execution of the filled-path playground example;
- IR/core: object-snapshot Transform round trips and backward-compatible stroke-style defaults;
- compiler: explicit geometry plans, safe filled-path acceptance, distinct `UnsafeFilledPathTransform` rejection, fill-presence rejection, stroke-width/join/cap safety boundaries, and unsupported cross-kind rejection;
- geometry/stroke: Lyon reference checks, cap/miter theory, contour-start/winding invariance, fixed topology, active-triangle winding, and all nine join/cap parity combinations;
- geometry/fill: rounded-loop -> concave-star topology, fill-only meshes, fill+stroke meshes, self-intersection/open-contour rejection, static-Lyon endpoint area fidelity, and a regression where both endpoints are individually valid but a fan triangle would invert only inside the animation interval;
- runtime: exact analytic/path endpoints, seek/forward parity, sequential continuity, precedence, reveal independence, path allocation stability, plus filled-path seek/forward parity and fill interpolation;
- renderer: stroke/fill-aware cache identity, analytic instance-only dirty ranges, filled-path cold-geometry reuse, no steady retessellation, and path instance-only morph updates;
- full CI: format, workspace compile, strict Clippy, stroke geometry suites, both filled-morph suites, all workspace tests, WebGPU wasm compile, browser-runtime wasm compile, and browser package validation.

## Current limitations / next work

Completed Transform geometry support now includes:

- same-kind analytic `Circle`, `Rectangle`, and `Line` interpolation;
- fixed-topology stroked vector-path Transform;
- shared round/miter/bevel joins and round/butt/square caps;
- bounded fixed-topology filled vector-path Transform for one continuously certifiable closed contour.

Remaining limitations include:

- general filled polygons outside the certified center-fan class;
- multiple filled contours / holes;
- cross-kind geometry interpolation;
- path stroke-width interpolation across geometry-changing morphs;
- changing join/cap topology during an active geometry-changing path Transform;
- `ReplacementTransform`, `TransformFromCopy`, and matching-shape variants.

The next Transform milestone should focus on lifecycle semantics: `ReplacementTransform` and `TransformFromCopy` first, then matching-shape correspondence. Those can build on the existing stable `ObjectId` plus detached `ObjectSnapshot` model without weakening the fixed-topology renderer contract.
