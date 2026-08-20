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

## Next implementation step

Integrate `MorphPlan` into compiled scene data and introduce a morph-progress timeline property that references a precomputed target plan. Then add a renderer representation whose topology stays fixed for the duration of the morph, so each frame updates point positions rather than retessellating the source path.

Correctness tests should cover:

- exact progress 0 and 1 correspondence endpoints;
- direct seek vs sequential playback parity;
- closed-path start-offset/winding normalization;
- no per-frame path flattening or correspondence work;
- stable renderer topology and bounded dirty uploads;
- browser/native parity.
