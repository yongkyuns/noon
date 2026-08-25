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

For backwards compatibility, historical Noon `set_fill(None)` / `set_stroke(None)` with no other arguments still explicitly disable that layer. In contrast, `set_fill(opacity=x)` and `set_stroke(width=..., opacity=...)` preserve the current color as Manim users expect.

## Pinned ManimCE v0.21.0 Cairo defaults

The raster oracle is pinned to ManimCE v0.21.0 with Cairo, so compatibility follows that renderer's effective contract rather than inventing a generic unit conversion:

- VMobject default `fill_opacity=0.0`;
- VMobject default `stroke_opacity=1.0`;
- default inherited paint color is white;
- default `stroke_width=4`;
- Cairo applies `cairo_line_width_multiple=0.01`, so the default stroke is **0.04 scene units** before camera projection;
- `joint_type=AUTO` and `cap_style=AUTO` leave Cairo's default **miter join** and **butt cap** in effect.

The Manim compatibility boundary therefore converts an authored Manim stroke width `w` to Noon's legacy render width `0.01 * w`. This translation is deliberately frontend-scoped: native Noon low-level style widths keep their existing units.

## Remaining stroke-scaling work

This conversion fixes the effective width for unscaled and rotated VMobjects, including the canonical Quickstart parity corpus. It is not the end of #179.

Manim applies Cairo stroke width after object point transforms, so ordinary object scaling does **not** multiply stroke thickness. Noon's current analytic and tessellated renderers interpret legacy `stroke_width` in local geometry units, so object scale still scales the stroke. Exact compatibility therefore still requires lowering `StrokeWidthMode::ScreenSpace` (or an equivalent transform-invariant width mode) through the renderer for scaled and non-uniformly-scaled objects.

That remaining renderer work must be tested independently from the 0.01 frontend conversion: changing camera size affects world-to-pixel projection in both systems, while changing object geometry scale must not change a Manim stroke's authored thickness.
