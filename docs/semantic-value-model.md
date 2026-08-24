# Semantic value, style, and bounds model

Noon's authoring semantics must not be constrained by the current 2D/f32 GPU
backend. Wave 1 therefore introduces an explicit high-precision semantic layer
while leaving compact renderer lowering as a separate concern.

## Numerics and 2.5D

`SemanticVec3` uses `f64` and retains x/y/z. The current 2D renderer lowers x/y
to `f32` explicitly; z remains available to semantic ordering and future
backends. `SemanticTransform2_5D` similarly keeps f64 translation/scale and a
z-axis rotation without pretending the renderer already supports full 3D.

This is sufficient for common Manim source that supplies three-component vectors
and z values while preserving a compact 2D execution/render path.

## Style

`SemanticStyle` separates:

- fill paint and fill opacity;
- stroke paint and stroke opacity;
- stroke width and its coordinate/scaling mode;
- overall object opacity.

Paint is extensible (`Solid` today, resource-backed paint later), so gradients do
not require changing the object style shape again. The legacy `Style` adapter
maps its global opacity to overall object opacity.

Stroke width has an explicit `ScaleWithObject` compatibility mode plus a
`ScreenSpace` mode rather than leaving scaling behavior implicit.

## Painter/z ordering

`SemanticPresentation` carries `z_index` and a stable insertion-order tie-break.
It is intentionally independent of style and transform hierarchy. The renderer
ordering PR (#54) can consume this semantic key without forcing storage/layout
choices into the renderer.

## Two classes of bounds

A single bounding box is not adequate for both layout and culling:

- **layout bounds** are tight enough for `next_to`, `align_to`, etc.;
- **conservative bounds** are cheap/safe for culling and spatial invalidation.

`semantic_path_bounds` computes both. Conservative path bounds use the Bezier
control hull. Layout bounds solve quadratic/cubic derivative roots and include
true curve extrema. Stroke expansion is explicit in both.

For example, a quadratic curve from `(0,0)` to `(2,0)` with control `(1,2)` has a
conservative max y of 2 but a true layout max y of 1. Tests lock in that
distinction.

## Migration boundary

This PR establishes semantic types and compatibility adapters; it does not force
every existing `Vec2`, `Transform2D`, `Style`, or renderer buffer to become f64.
The stable semantic store (#51) can adopt these values independently, while
compiled/runtime/render data may continue using f32 after validated lowering.
