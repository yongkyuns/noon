# Manim style compatibility semantics

This note records the style decisions behind the ManimCE v0.21.x compatibility work. It supplements `manim-aligned-authoring-plan.md` and keeps compatibility changes aligned with Noon's existing renderer-independent semantic model.

## Independent fill and stroke opacity

Manim exposes independent `fill_opacity` and `stroke_opacity`. Noon does not need new serialized style fields to represent that distinction: `Style.fill` and `Style.stroke` already carry independent RGBA `Color` values.

The compatibility mapping is therefore:

```text
Manim fill_color + fill_opacity
        -> Noon Style.fill RGBA

Manim stroke_color + stroke_opacity
        -> Noon Style.stroke RGBA

Noon overall opacity
        -> Style.opacity multiplier
```

This has useful architectural properties:

- no `SceneDefinition` format-version change;
- no compiler/runtime property expansion;
- no GPU instance-layout change;
- no shader-interface change;
- no path-cache identity change;
- generic Transform already interpolates each color's alpha independently;
- existing low-level overall-opacity animation remains available as a Noon extension.

The public compatibility layer accepts constructor keywords `fill_color`, `fill_opacity`, `stroke_color`, and `stroke_opacity`. `set_fill(..., opacity=...)` changes only fill alpha; `set_stroke(..., opacity=...)` changes only stroke alpha; Manim-style `VMobject.set_opacity(...)` updates both layer alphas.

`VMobject.set_color(...)` changes the RGB of both active paint layers while preserving each layer's existing alpha. This is important for default VMobjects: recoloring a shape must not turn the normally transparent fill opaque.

For backwards compatibility, historical Noon `set_fill(None)` / `set_stroke(None)` with no other arguments still explicitly disable that layer. In contrast, `set_fill(opacity=x)` and `set_stroke(width=..., opacity=...)` preserve the current color as Manim users expect.

## Pinned ManimCE v0.21.0 Cairo defaults

The raster oracle is pinned to ManimCE v0.21.0 with Cairo, so compatibility follows that renderer's effective contract rather than inventing a generic unit conversion:

- VMobject default `fill_opacity=0.0`;
- VMobject default `stroke_opacity=1.0`;
- default inherited paint color is white;
- default `stroke_width=4`;
- Cairo applies `cairo_line_width_multiple=0.01`, so the default stroke is **0.04 scene units** before camera projection;
- `joint_type=AUTO` and `cap_style=AUTO` leave Cairo's default **miter join** and **butt cap** in effect.

The Manim facade therefore constructs its VMobject families with those defaults and converts an authored Manim stroke width `w` to Noon's legacy render width `0.01 * w`. The shared low-level IR emitter remains neutral, so native Noon constructors and Rust/Python cross-language parity retain Noon's existing style defaults and units.

## Object-transform-invariant stroke lowering

The Manim facade marks converted Cairo widths with `StrokeWidthMode::ScreenSpace`. In this compatibility mode, “screen space” means that object scale and rotation are applied to the VMobject points **before** the stroke is constructed; the width itself remains the converted scene-space quantity (`4 -> 0.04`) and therefore still changes pixel thickness when the camera projection changes. Native Noon styles remain `ScaleWithObject`.

The renderer lowers this contract without changing the packed instance size. Analytic line strokes are widened after transforming their centerline into scene space; circle/rectangle SDF strokes compensate local width by the object transform; and vector paths bake scale/rotation into their contour before tessellation, then render the resulting mesh with translation only. This makes uniform and non-uniform object scaling stop multiplying Manim stroke thickness while preserving the existing fast transform-scaled path for native Noon.

Transform-invariant vector paths currently retessellate when their scale or rotation changes because exact non-uniform Cairo semantics require stroking the transformed contour. A future GPU path-extrusion representation can recover transform-only animation reuse without weakening parity. #179 still owns remaining cap/join/endpoint/AA convergence.
