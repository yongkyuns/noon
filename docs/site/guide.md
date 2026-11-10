# Scene authoring

Start with the [Python](python.html) or [Rust](rust.html) walkthrough. This chapter
connects the common authoring concepts to the reference and runnable examples.

## Scene, objects, and layout

A scene holds authored objects and their relationships. A shape declaration is
not a DOM element. Arrangement methods such as `next_to`, `shift` and grouping
operate in scene coordinates through the shared semantic engine. The web page's
CSS size affects presentation, not the authored geometric coordinates.

Use families when several shapes should share an animation. Keep permanent
scene edits distinct from effective values produced by an active animation or
updater; this distinction matters when querying live state and adding callbacks.
The [implementation reference](reference.html#authoring) explains these boundaries.

## Introduce and animate objects

```python
from noon import *

class Demo(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square(color=PINK).next_to(circle, RIGHT)
        self.play(Create(circle), Create(square))
        self.play(circle.animate.shift(UP),
                  square.animate.rotate(PI / 4), run_time=2)
        self.wait(1)
```

Animations passed to one ordinary `play` run together; successive calls express
successive segments. `wait` advances authored time. In Rust, tuples, arrays and
vectors can group animation requests; use the documented timing options rather
than manually driving wall-clock time.

## Text, formulae, plots and code

The first lesson uses `Text` and `MathTypst`. Typst and LaTeX are separate paths:
do not assume a LaTeX expression can be passed unchanged to a Typst constructor.
Read the [implementation reference](reference.html) for text resources,
`Tex`/`MathTex` preparation, plotting, code blocks, images and supported profiles.
The generated [Python API](api.html#python) links the current constructors and
their source declarations.

## Paints, shaders and effects

Paints determine a surface's appearance; effects may operate on a rendered
object or group. The supported effect profile and compositing order matter.
Start with a qualified example rather than applying a browser-only API to a
native Python scene without checking its host support.

[Interference is addition](examples.html#interference-is-addition) uses analytic
wave fields and a shared shader material; it is not a numerical wave simulation.
[Make it move](examples.html#make-it-move) combines type, animated color, grain and
backdrop effects. Full details remain in
[paints, effects and shader materials](reference.html#paints-effects-and-shader-materials).

## Spatial scenes and cameras

Spatial geometry, world transforms and cameras use the shared Rust operations,
not a separate scene engine. Coverage differs by host and feature combination.
Consult the spatial sections in [the reference](reference.html), then run the
paired source example on the actual target before relying on a capability.

## Callbacks and interaction

Ordinary code computes values during authoring. An updater or input-driven action
may instead require host code while the scene is running. That can restrict
preparation, seeking or export. Do not promise deterministic replay for an
unrecorded input-dependent scene. See [playback and interaction](playback.html).
