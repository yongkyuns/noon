# ManimCE source-compatibility plan

## Goal

Noon's Python authoring surface should target **source compatibility with the common 2D Manim Community API**, while keeping Noon's own deterministic semantic model, compiled Rust/WASM runtime, analytic primitives, retained geometry caches, WebGPU/WebGL2 renderers, and live patching architecture.

The practical target is:

```python
# Typical ManimCE source
from manim import *

class Demo(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square().set_fill(PINK, opacity=0.5)
        square.next_to(circle, RIGHT)

        self.play(Create(circle))
        self.play(Create(square))
        self.play(
            circle.animate.shift(UP),
            square.animate.rotate(PI / 4),
            run_time=2,
            rate_func=smooth,
        )
```

should require, wherever Noon supports the involved feature, only the import change:

```python
from noon import *
```

The compatibility reference is **Manim Community v0.21.x**. Compatibility means public names, constructor shapes, method names, common defaults, lifecycle behavior, and observable authoring semantics. It does **not** mean copying Manim's Cairo/OpenGL implementation or storing every object as a Manim-style point cloud.

## Architecture invariant

Compatibility is a frontend contract:

```text
Manim-compatible Python authoring
              |
              v
      Noon semantic snapshots
              |
              v
    SceneDefinition / ScenePatch
              |
              v
        compiler / tracks
              |
              v
       Rust/WASM runtime
              |
              v
      WebGPU / WebGL2
```

Examples:

- `Circle` may be a Python `VMobject` subclass while still lowering to Noon's analytic `GeometryRef::Circle`.
- `VGroup.animate.shift(RIGHT)` may lower to parallel member tracks without introducing runtime hierarchy.
- `rate_func=smooth` may lower to a known deterministic easing representation rather than execute Python every frame.
- `Create(Circle())` may auto-bind the detached object during authoring while preserving the same canonical lifecycle/reveal tracks.

The compatibility effort must not introduce a second serialized scene model or require Python callbacks during steady-state playback.

## Current compatibility assessment

Noon already has substantial vocabulary compatibility:

- `Scene`, `Mobject`, `Group`, `VGroup`
- `Circle`, `Rectangle`, `Square`, `Line`, `Path`
- `Create`, `FadeIn`, `FadeOut`
- `Transform`, `ReplacementTransform`, `TransformFromCopy`, `TransformMatchingShapes`
- `.animate`
- `shift`, `move_to`, `scale`, `rotate`
- `next_to`, `align_to`, `to_edge`, `to_corner`
- `set_color`, `set_fill`, `set_stroke`, `set_opacity`
- direction constants, angle constants, layout buffers, and much of the Manim color palette
- `Scene.play(..., run_time=...)` and `Scene.wait(...)`

However, Noon is currently **Manim-inspired rather than Manim-source-compatible**. The largest mismatches are structural:

1. the playground historically required an explicit `result = Scene()` style instead of normal `class Foo(Scene): construct()` authoring;
2. shape constructors are functions returning a generic `Mobject`, so class identity/subclassing does not match Manim;
3. introducer animations such as `Create(Circle())` historically require pre-binding the object to a scene;
4. Noon exposes string `easing=` while Manim users expect `rate_func=` and named rate functions;
5. Noon vectors are 2-tuples while Manim commonly uses 3-component vectors with `z == 0` for 2D;
6. `Group`/`VGroup` are authoring collections rather than full Mobject-family objects;
7. style storage currently has one overall opacity rather than independent fill/stroke opacity;
8. text/MathTex, animation composition, axes/plotting, and much of VMobject are not yet present;
9. arbitrary Python per-frame updaters conflict with Noon's compiled playback model.

## Compatibility levels

### Level 1 — core 2D Manim syntax

Target near-source-compatibility for the most common scene-building surface:

- scene model: `Scene`, `construct`, `setup`, `tear_down`, `add`, `remove`, `clear`, `mobjects`, `play`, `wait`
- object model: `Mobject`, `VMobject`, `Group`, `VGroup`
- shapes: `Circle`, `Rectangle`, `Square`, `Line`, `Dot`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Triangle`, `Arrow`, `Vector`
- creation/fading/transforms: `Create`, `Uncreate`, `FadeIn`, `FadeOut`, `Transform`, `ReplacementTransform`, `TransformFromCopy`
- object operations: `move_to`, `shift`, `scale`, `rotate`, `next_to`, `align_to`, `to_edge`, `to_corner`, `set_x`, `set_y`, style setters and bounds queries
- `.animate`
- direction/color/angle/buffer constants
- named rate functions and `rate_func=`

### Level 2 — typical mathematical animation

Add the APIs needed by a large share of educational Manim scenes:

- `Text`, `MarkupText`, `Tex`, `MathTex`
- `Write`, `Unwrite`, `DrawBorderThenFill`
- `AnimationGroup`, `Succession`, `LaggedStart`
- `Axes`, `NumberPlane`, `NumberLine`, `FunctionGraph`, `ParametricFunction`
- `ValueTracker`, `DecimalNumber`
- `Brace`, `SurroundingRectangle`, `DashedLine`

Text should use Noon's intended semantic architecture: shaped glyph runs/atlas for normal steady-state text, with vector outlines when path-level animation requires them.

### Level 3 — rich vector authoring

Add broader VMobject/source compatibility:

- `VMobject.start_new_path`
- `add_line_to`
- `add_cubic_bezier_curve_to`
- `set_points_as_corners`
- `point_from_proportion`
- `SVGMobject`, `ImageMobject`
- richer matching transforms including text-aware matching

### Level 4 — intentionally difficult compatibility

These APIs depend on imperative per-frame Python execution in Manim:

- `Mobject.add_updater(lambda m, dt: ...)`
- arbitrary `Scene.add_updater`
- arbitrary Python `rate_func`
- `always_redraw` backed by arbitrary Python
- `UpdateFromFunc` and similar callback animations
- camera internals and 3D scene behavior

Do not weaken Noon's architecture just to report these as supported. Prefer deterministic compiled equivalents where possible (`ValueTracker`, declarative expressions, known rate functions), and otherwise raise a precise unsupported-feature error.

## Compatibility rules

1. **Match syntax where semantics fit.** Use Manim names, positional/keyword argument conventions, and lifecycle behavior when Noon can represent them faithfully.
2. **Keep analytic/render fast paths.** Python inheritance must not force analytic shapes into generic paths.
3. **Use one semantic authority.** Python compatibility wrappers lower into the same canonical snapshots/tracks used by Rust and other frontends.
4. **Prefer explicit unsupported errors over silent semantic drift.** A Manim call that Noon cannot represent should fail clearly.
5. **Preserve low-level Noon escape hatches.** Explicit track timing, raw paths, scene patches, and renderer diagnostics remain available outside the normal Manim-compatible surface.
6. **Test source compatibility, not only Noon-specific examples.** CI should contain small programs whose body is valid ManimCE code and differs only in the import line.
7. **Treat compatibility as versioned.** The matrix is anchored to ManimCE 0.21.x and should record intentional deviations.

## High-priority semantic mismatches

### Scene authoring

Support canonical Manim form:

```python
class Example(Scene):
    def construct(self):
        self.play(Create(Circle()))
```

The browser runner should discover scene subclasses, instantiate one deterministically, call `setup()`, `construct()`, and `tear_down()`, then compile the resulting Noon scene. Existing explicit `result = scene` scripts remain supported as a low-level/backwards-compatible mode.

### Real shape classes

Move from constructor functions returning a generic object to a public hierarchy resembling:

```text
Mobject
  └─ VMobject
      ├─ Circle
      ├─ Rectangle
      │   └─ Square
      ├─ Line
      └─ Path
```

This is required for `isinstance`, subclassing, copy/type preservation, discoverability, typing, and future shape-specific methods. It does not imply Manim's internal point representation.

### Introducer lifecycle

`Create`, `FadeIn`, and future introducing animations should accept detached objects. Authoring should bind them to the scene automatically when the animation is played.

### Vectors

Keep a compact 2D semantic representation internally, but accept common Manim/Python inputs:

- `(x, y)`
- `(x, y, 0)`
- lists with the same forms
- NumPy-like indexable vectors when available

A nonzero Z component should produce a clear 3D-not-supported error until Noon gains 3D semantics.

### Rate functions

Expose Manim vocabulary such as `linear`, `smooth`, `rush_into`, `rush_from`, `there_and_back`, and common easing names. Known functions lower to deterministic runtime easing identifiers/curves. `Scene.play(..., rate_func=smooth)` is the public form; `easing=` remains a low-level/backwards-compatible alias.

### Style model

Move the canonical style toward separate:

```text
fill_color
fill_opacity
stroke_color
stroke_opacity
stroke_width
stroke_join
stroke_cap
overall_opacity (optional multiplier)
```

so `set_fill(..., opacity=...)` does not alter stroke opacity. This requires coordinated IR/compiler/runtime migration and should be done as its own compatibility milestone.

### Groups

`Group` and `VGroup` should participate in Mobject-family authoring semantics while remaining cheap authoring-time collections if runtime hierarchy is unnecessary. Group transforms and animations can lower to member operations/tracks.

## Delivery sequence

### Compatibility Phase A — source-compatible foundation

Implement first:

- canonical `Scene.construct()` execution in the browser runner;
- real public `VMobject` and shape classes without changing renderer representation;
- detached-object `Create` / `FadeIn` introduction;
- 2D acceptance of 3-component Manim vectors;
- `rate_func=` plus common named deterministic rate functions;
- compatibility regression scripts using normal Manim-style scene bodies.

Acceptance:

- a representative `class Demo(Scene): construct()` script runs in the playground without assigning `result`;
- `isinstance(Circle(), Circle)` and `isinstance(Circle(), VMobject)` are true;
- `self.play(Create(Circle()))` works;
- `RIGHT + UP` and `(1, 2, 0)` inputs work while nonzero Z is rejected clearly;
- `self.play(..., rate_func=smooth)` lowers to deterministic easing;
- all existing Noon Python examples and both WebGPU/WebGL2 browser suites remain green.

### Compatibility Phase B — scene/group/style parity

- Manim-style `Scene.add/remove/clear/mobjects` behavior;
- `Group`/`VGroup` as full authoring Mobjects;
- split fill/stroke opacity and align style keyword/default behavior;
- generalize `.animate` method proxying;
- add `Animation` base metadata and common per-animation options.

### Compatibility Phase C — animation composition and core shape breadth

- `AnimationGroup`, `Succession`, `LaggedStart`;
- `Uncreate`, `Write` groundwork, growing/indication basics where deterministic;
- `Dot`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Triangle`, `Arrow`, `Vector`, `DashedLine`.

### Compatibility Phase D — text and mathematical authoring

- `Text`, `MarkupText`, `Tex`, `MathTex`;
- glyph shaping/cache/atlas steady-state path;
- outline extraction for `Create`/`Write`/matching transforms;
- `TransformMatchingTex`.

### Compatibility Phase E — plotting and values

- axes/number lines/planes;
- graphs and parametric functions;
- trackers and declarative value-driven expressions;
- braces, labels, decimal numbers.

### Compatibility Phase F — advanced VMobject/import surface

- public path-building VMobject APIs;
- SVG/image import;
- broader compatibility corpus and documented intentional gaps.

## CI compatibility corpus

Add a dedicated compatibility suite. Each fixture should look like ordinary Manim source except for the import:

```python
from noon import *

class BasicCreate(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square().next_to(circle, RIGHT)
        self.play(Create(circle), Create(square))
        self.play(circle.animate.shift(UP), run_time=2, rate_func=smooth)
```

Tests should check:

- source executes successfully;
- semantic object/track counts and lifecycle are correct;
- final state matches expected geometry/style;
- direct seek agrees with forward playback;
- browser visual smoke remains valid on WebGPU and WebGL2;
- unsupported features fail with stable, specific messages.

## Execution status

The original Manim-aligned migration (legacy cleanup, semantic layout, scene cursor, `.animate`, generic Transform, and gallery cleanup) is complete and remains the architectural baseline.

**Compatibility Phase A is now the active implementation milestone.** Subsequent sections of this document should be updated with concrete PRs/commits and any deliberate deviations as they land.

## Non-goals

- copying Manim's renderer or scene internals;
- replacing analytic primitives with generic paths solely for API compatibility;
- running arbitrary Python every frame in the normal playback path;
- claiming unsupported 3D/camera/updater behavior works;
- creating a second persistent authoring IR.
