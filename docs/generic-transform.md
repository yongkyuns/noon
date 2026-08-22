# Generic Transform architecture

## Status

Generic `Transform` is a first-class semantic animation in the realtime/browser architecture. It is not implemented as a collection of frontend-only position/style tracks and it does not encode morph targets by mutating the source geometry in authoring IR.

The language-neutral contract remains:

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

This keeps compatible primitives analytic, precomputes vector correspondence, and makes unsupported topology changes explicit.

## Authoring model

Python and Rust author the same semantic operation. Detached objects are valid Transform targets without becoming rendered scene objects:

```python
circle = scene.add(Circle(1.0, color=BLUE))
scene.play(
    Transform(circle, Square(2.0, color=PURPLE)),
    run_time=2.0,
    easing="ease_in_out_cubic",
)
```

The serialized `SceneDocument` still contains a semantic Circle source snapshot and Rectangle/Square target snapshot. The frontend does not convert either object to a path.

Rust uses the same model through the facade:

```rust
use noon::prelude::*;

let mut scene = Scene::new();
let circle = scene.add(Circle::new(1.0).color(BLUE));
scene
    .play(Transform::new(circle, Square::new(2.0).color(PURPLE)))
    .run_time(2.0)?;
```

Targets are copied when scheduled. Sequential Transforms chain evaluated semantic snapshots. Python bound `Mobject` reads are evaluated at the current scene cursor, so a later `.animate` begins from the previous animation endpoint rather than the original base object.

## Compiler lowering

The compiler retains the semantic `from`/`to` snapshots and creates a `TransformGeometryPlan` describing execution.

### Same geometry

If source and target geometry are identical, the compiler emits `TransformGeometryPlan::Static`. Transform and supported style channels interpolate without a renderer geometry override.

### Same-kind analytic primitives

These remain fully analytic:

- `Circle -> Circle`: radius interpolation;
- `Rectangle/Square -> Rectangle/Square`: size interpolation;
- `Line -> Line`: endpoint interpolation.

The runtime writes these values directly into semantic `FrameObjectState.geometry`. No path conversion, Lyon tessellation, path-cache entry, or path geometry upload is involved.

### Vector path -> vector path

Geometry-changing path Transforms use deterministic feature-preserving correspondence and fixed source/target GPU geometry. Correspondence and topology work happen before steady playback. The renderer receives one prepared path pair and a normalized morph parameter.

### Circle <-> Rectangle/Square

Closed analytic cross-kind Transforms are supported without weakening the steady-state analytic representation.

The compiler converts only the temporary transition geometry:

- a Circle becomes a closed four-cubic Bezier path using the standard `0.5522848 * radius` control distance;
- a Rectangle becomes a closed path containing corners and side midpoints;
- both temporary paths enter the existing deterministic fixed-path morph planner;
- the resulting prepared pair is stored only in `TransformGeometryPlan::PathPair`.

The semantic track is **not rewritten**. Before completion the semantic object remains the exact Circle source; at completion it becomes the exact Rectangle target. `FrameState::render_geometries` carries the prepared path pair only for rendering the transition.

This means:

- static Circles and Rectangles remain analytic;
- same-kind analytic Transform remains analytic;
- a cross-kind Transform pays path preparation only for that transition;
- direct seek and forward playback share the same fixed prepared geometry;
- scene identity and serialized semantics remain unchanged.

Circle <-> Line and Rectangle <-> Line are intentionally still unsupported because they cross the closed/open topology boundary. They are rejected during compilation rather than silently choosing an arbitrary collapse rule.

## Path topology and fill safety

Supported semantic stroke policies are:

- joins: `round`, `miter`, `bevel`;
- caps: `round`, `butt`, `square`.

For a geometry-changing PathPair, stroke width, join, cap, and fill presence are part of mesh topology/cache identity and cannot change during the same Transform.

Filled path Transform is supported for a bounded class. The planner requires:

1. source and target each contain exactly one closed contour;
2. resampled endpoint contours are simple and nondegenerate;
3. fill is enabled at both endpoints or disabled at both endpoints;
4. stroke width, join, and cap topology remain unchanged;
5. one center-fan triangulation remains valid over the complete interpolation interval.

The final condition is proved continuously rather than frame-sampled. Each moving fan triangle has a quadratic signed-area function over time; the planner evaluates its endpoints and exact interior critical point. If a triangle cannot be certified positive throughout `[0, 1]`, compilation rejects the Transform with `UnsafeFilledPathTransform`.

The same certifier is used by Circle <-> Rectangle/Square cross-kind lowering. The supported closed analytic pair therefore goes through the same fill safety boundary as authored vector paths.

No correspondence, fill triangulation, or tessellation runs per frame.

## Runtime semantics

`FrameObjectState` remains semantic.

For same-kind analytic primitive Transforms, geometry itself is interpolated at every evaluated time and `FrameState::render_geometries` stays empty.

For PathPair Transforms, including Circle <-> Rectangle/Square:

- semantic geometry is the exact source endpoint before completion;
- semantic geometry becomes the exact target endpoint at completion;
- renderer-prepared fixed-topology source/target geometry is stored separately;
- morph progress is independent from Reveal;
- transform/style interpolation is deterministic under direct seek and forward playback.

Narrow property tracks are applied after the generic Transform group, so explicit Position, Rotation, or Opacity tracks override their corresponding channels while the remaining Transform channels continue normally.

## Lifecycle composition

`ReplacementTransform`, `TransformFromCopy`, and matching-shape handoffs build on the same semantic Transform contract plus explicit zero-duration `Presence` events. They do not require renderer-specific object insertion/deletion.

Lifecycle source/target snapshots are evaluated at semantic start/handoff times when state can be represented exactly by `ObjectSnapshot`. Ambiguous active generic Transform state and non-snapshot lifecycle channels are rejected instead of silently approximated.

## Performance contract

For an active same-kind analytic Transform:

- geometry remains analytic;
- no path conversion/tessellation occurs;
- no path mesh is inserted into the cache;
- a changed object dirties only its analytic instance record.

For an active PathPair Transform, including supported cross-kind analytic Transforms:

- correspondence is computed before steady playback;
- stroke topology and fill triangulation are not recomputed per frame;
- tessellation is not rerun per frame;
- geometry buffers are not re-uploaded per frame;
- the fixed source/target path mesh is cached;
- steady frames dirty only the path instance record.

The cross-kind conversion is therefore a compiler-selected transition strategy, not a permanent representation change.

## Explicit safety boundaries

The compiler rejects rather than silently degrading when a Transform leaves the supported contract:

- unsafe/self-intersecting/non-certifiable filled path geometry;
- multiple or open contours for a filled geometry-changing path Transform;
- path fill-presence changes;
- path stroke-width changes;
- join/cap topology changes;
- unsupported open/closed cross-kind transforms such as Circle -> Line;
- unsupported external/custom geometry without a declared interpolation strategy.

## Validation

Coverage is split across independent layers:

- Python: semantic Circle -> Square authoring remains Circle/Rectangle snapshots in `SceneDocument`; chained `.animate` begins from the prior semantic endpoint;
- Rust facade: the same Circle -> Square authoring lowers to `TrackValues::Object` with analytic semantic endpoints;
- compiler: Circle <-> Rectangle selects `PathPair`, retains original semantic snapshots, same-kind primitives retain analytic plans, and open/closed cross-kind geometry is rejected;
- geometry: the existing fixed-topology morph and continuous filled-morph certification suites validate the temporary path pair;
- runtime: Circle -> Rectangle keeps exact semantic endpoints, exposes renderer-only path geometry during the transition, and direct seek matches forward playback;
- renderer: existing PathPair cache/dirty-range tests cover fixed geometry reuse and instance-only steady updates;
- full CI: format, workspace compile, strict Clippy, geometry suites, all workspace tests, WebGPU wasm, browser-runtime wasm, and browser package validation.

## Current limitations / next work

Supported Transform behavior now includes:

- same-kind analytic Circle, Rectangle, and Line interpolation;
- Circle <-> Rectangle/Square compiler-only path lowering;
- fixed-topology stroked vector-path Transform;
- bounded fixed-topology filled vector-path Transform;
- stable-ID lifecycle composition;
- evaluated authoring snapshots and sequential `.animate` chaining.

Remaining limitations include:

- Circle/Rectangle <-> Line cross-kind interpolation;
- general filled polygons outside the certified center-fan class;
- multiple filled contours / holes;
- path stroke-width interpolation across geometry-changing morphs;
- changing join/cap topology during an active geometry-changing path Transform;
- external/custom geometry without an explicit lowering strategy.

Future cross-kind support should extend explicit compiler strategies rather than introducing a universal permanent path representation.
