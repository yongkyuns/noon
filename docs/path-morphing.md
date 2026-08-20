# Path morphing design

Noon path morphing is defined on semantic path centerlines, not on renderer tessellation.

That distinction is important: Lyon stroke meshes contain duplicated/extruded vertices whose topology depends on stroke width, joins, caps, and tessellation tolerance. Treating those triangles as animation correspondence would make morph semantics renderer-dependent and would force retessellation artifacts into the authoring model.

## Current implemented slice

`noon-geometry::plan_morph` precomputes deterministic point correspondence between two `VectorPath` values.

The planner:

1. validates both semantic paths;
2. flattens line/quadratic/cubic commands into ordered centerline polylines with deterministic adaptive subdivision;
3. requires equal contour counts;
4. requires each corresponding contour to agree on open/closed topology;
5. arc-length resamples each corresponding contour to the same fixed number of points;
6. preserves authored direction for open paths;
7. for closed contours, searches cyclic start offsets and reversed winding and chooses the minimum-displacement deterministic correspondence;
8. stores source/target point arrays so frame-time interpolation is only `lerp` over precomputed points.

Default planning currently uses 64 points per contour and a 0.01-world-unit flattening tolerance. Both are explicit `MorphOptions` so the compiler can choose a quality/performance profile later.

## Compatibility contract

The first morph implementation intentionally rejects ambiguous topology instead of silently inventing correspondence.

Compatible:

- one open contour -> one open contour;
- one closed contour -> one closed contour;
- N contours -> N contours when each corresponding contour has the same closure state;
- different command counts and curve types, because both sides are normalized by centerline flattening + arc-length resampling;
- closed paths with different starting vertices or opposite winding.

Rejected:

- different contour counts;
- open -> closed contour changes;
- zero-length contours;
- malformed/non-finite paths;
- invalid sampling/tolerance options.

Contour reordering is not inferred yet. The current contract pairs contours by semantic order. Future shape-matching heuristics can be added as an explicit planning policy rather than silently changing default identity semantics.

## Determinism

Given identical source path, target path, and `MorphOptions`, `plan_morph` must return byte-for-byte equivalent Rust values.

Closed-contour alignment uses a deterministic exhaustive search over forward/reversed winding and cyclic shifts. Iteration order is the tie-breaker: forward winding is considered before reversed winding, and lower shifts are considered first.

## Runtime/render architecture

The intended next seam is:

```text
VectorPath source + target
        |
        v
plan_morph()                 authoring/compile time
        |
        v
MorphPlan
  source_points[]
  target_points[]
  contour metadata
        |
        v
morph progress scalar        frame time
        |
        v
GPU/CPU point interpolation
        |
        v
stable morph mesh/render path
```

The runtime should not rebuild semantic paths or run curve flattening each frame. `MorphFrame::to_vector_path()` exists only as a reference/debug CPU representation; normal playback should interpolate the precomputed correspondence directly.

## Playable renderer slice

The first playable morph renderer is implemented as a fixed-topology stroke mesh. Each morph vertex stores both its source and target position; the WebGPU vertex shader interpolates them from normalized morph progress. The index buffer and vertex correspondence remain unchanged for the whole animation. Normal paths keep using the same path pipeline, with identical source/target positions.

The runtime reuses the existing normalized path scalar channel: ordinary paths interpret it as reveal, while paths with a semantic `morph_target` interpret it as morph progress. This keeps frame state compact and means morph playback changes only the path instance record; geometry is not retessellated or re-uploaded each frame. The Python API exposes this as `scene.animate_morph(path, target, ...)`.

Current intentional boundary: morph rendering is stroke-only. Fill triangulation during topology-changing interpolation is deferred until a stable fill strategy is selected. A path cannot currently animate reveal and morph simultaneously because those operations share the normalized path channel.

## Next implementation step

Next, add fill morphing and decide whether reveal+morph composition warrants separate scalar channels. The stroke morph path is already fixed-topology and GPU-interpolated, so normal playback performs no path planning, tessellation, or geometry upload per frame.

Correctness tests should cover:

- exact progress 0 and 1 correspondence endpoints;
- direct seek vs sequential playback parity;
- closed-path start-offset/winding normalization;
- no per-frame path flattening or correspondence work;
- stable renderer topology and bounded dirty uploads;
- browser/native parity.
