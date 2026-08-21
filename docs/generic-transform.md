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

## Python authoring model

Detached objects are valid Transform targets without becoming rendered scene objects:

```python
source = scene.path(
    source_path,
    fill=None,
    stroke=WHITE,
    stroke_width=0.1,
)

target = Path(
    target_path,
    position=(2.0, -1.0),
    rotation=0.5,
    scale=(1.5, 0.75),
    fill=None,
    stroke=BLUE,
    stroke_width=0.1,
    opacity=0.6,
)

scene.play(
    Transform(source, target),
    duration=2.0,
    easing="ease_in_out_cubic",
)
```

`Transform(source, VectorPath(...))` remains a convenience form. It snapshots the current/source transform and style and replaces only the target geometry.

Targets are copied when scheduled. Later mutation of a detached target does not change an already-authored Transform.

Sequential Transforms on one Python object chain snapshots: the previous target becomes the next source snapshot. Overlapping generic Transforms for the same object are currently rejected by the Python authoring layer because precedence between two whole-object snapshots would otherwise be ambiguous.

## Compiler lowering

The compiler keeps the semantic `from`/`to` snapshots on one compiled Transform track and may additionally create renderer-only prepared geometry.

### Same geometry

If source and target geometry are identical, no renderer geometry override is needed. Transform, scale, rotation, fill/stroke color, opacity, and other supported style channels interpolate from the snapshots.

### Vector path -> vector path

Geometry-changing path Transforms use the existing deterministic path correspondence planner and fixed-topology dual-position mesh. Correspondence/tessellation work happens before steady playback. The renderer receives one prepared source/target geometry pair and a normalized morph parameter.

Current safety boundary:

- path stroke width must remain constant across a geometry-changing Transform;
- geometry-changing filled paths are rejected because the current fixed-topology morph mesh is stroke-only;
- unsupported cross-geometry Transforms are rejected before runtime.

This is intentional. Noon must not silently fall back to per-frame Lyon tessellation.

## Runtime semantics

`FrameObjectState` remains semantic. Its geometry is the exact source endpoint before completion and the exact target endpoint at completion; renderer-prepared morph geometry is stored separately in `FrameState::render_geometries`.

This separation prevents a GPU optimization artifact such as `source.with_morph_target(target)` from leaking into the renderer-independent scene state.

Transform/style interpolation is deterministic under direct seek and forward playback. Narrow property tracks are applied after the generic Transform group, so an explicit `Position`, `Rotation`, or `Opacity` track overrides that corresponding channel while the remaining Transform channels continue normally.

Path Transform progress and path reveal remain independent.

## Performance contract

For an active geometry-changing path Transform:

- correspondence is not recomputed per frame;
- path tessellation is not rerun per frame;
- geometry buffers are not re-uploaded per frame;
- the fixed path mesh is cached;
- steady frames dirty only the path instance record;
- semantic and renderer path allocations are reused across steady forward frames.

The runtime only clones semantic or prepared geometry when the selected geometry actually changes, such as entering a different sequential Transform pair or reaching a semantic endpoint. A regression test compares the underlying path command-buffer addresses across successive steady frames so accidental deep cloning becomes a structural test failure.

## Validation

Coverage is split across independent layers:

- Python: detached targets, snapshot-by-value behavior, stable source identity, multiple and sequential Transforms, overlap rejection, old VectorPath convenience syntax;
- IR: object-snapshot Transform round trips;
- compiler: prepared path pair creation, same-geometry fast path, unsupported geometry rejection, fill/stroke-width safety boundaries;
- runtime: exact endpoints, seek/forward parity, sequential boundary continuity, property precedence, reveal independence, allocation stability;
- renderer: no steady retessellation/cache miss, instance-only dirty ranges, one-time prepared-geometry switch between sequential path pairs;
- full CI: format, workspace compile, strict Clippy, geometry correctness, all workspace tests, WebGPU wasm compile, browser-runtime wasm compile, and browser package validation.

## Current limitations / next work

The current generic Transform milestone deliberately does not yet provide:

- circle radius -> circle radius interpolation;
- rectangle size -> rectangle size interpolation;
- line endpoint -> line endpoint interpolation;
- cross-kind geometry interpolation;
- filled path morphing;
- path stroke-width interpolation;
- shared round/miter/bevel join and cap semantics between static and morph paths;
- `ReplacementTransform`, `TransformFromCopy`, or matching-shape variants.

The next generic-Transform slice should add same-kind analytic primitive geometry interpolation (`Circle`, `Rectangle`, `Line`) without converting primitives to paths. After that, path join/cap parity and endpoint visual-regression coverage should close the remaining stroke-rendering fidelity gap.
