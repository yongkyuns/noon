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

## Stroke-width units: unresolved semantic mismatch

ManimCE v0.21 uses a default VMobject `stroke_width=4`, and ordinary `VMobject.scale(...)` does not scale the stroke unless `scale_stroke=True` is requested. This makes stroke width behave like a display-space style quantity rather than ordinary local geometry.

Noon's current analytic and tessellated paths use `stroke_width` in local/world geometry units. Object scale therefore naturally scales stroke geometry. Simply passing Manim values such as `4`, `8`, or `20` through unchanged would produce grossly incorrect visuals, while silently applying a fixed conversion constant would only approximate one camera/frame configuration.

Until this is resolved, the compatibility layer should not claim numeric Manim stroke-width parity. Existing Noon world-space stroke widths remain supported, and independent stroke opacity can land without choosing a stroke-width policy.

The two coherent long-term options are:

1. **Display-space Manim stroke widths.** Add an explicit display-space stroke-width semantic to the renderer. This gives stronger visual/source compatibility but requires renderer work for analytic primitives and vector paths, including non-uniform transforms and cache/tessellation policy.
2. **World-space Noon stroke widths with a compatibility conversion.** Keep the simpler current renderer model and translate Manim-style widths at the Python boundary. This preserves the current fast geometry architecture but cannot exactly match Manim across camera/output-scale changes.

This is a real semantic choice, not merely an API spelling issue, and should be decided before changing compatibility defaults from Noon's existing stroke-width behavior.
