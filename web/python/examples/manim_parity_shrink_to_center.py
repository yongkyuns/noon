# Source-equivalent ManimCE v0.21.0 ShrinkToCenter parity candidate.
# Canonical fixture: shrink-to-center-rectangle
# The upstream reference example uses Text, which is outside Noon's current exact
# text subset. This geometry scene is ordinary Manim source and intentionally matches
# the output-affecting code in the canonical geometry fixture.

from noon import *

scene = Scene()
rectangle = Rectangle(
    width=2.0,
    height=1.0,
    fill_color=GREEN,
    fill_opacity=1.0,
    stroke_opacity=0.0,
).shift(RIGHT * 1.25)
scene.play(ShrinkToCenter(rectangle))
result = scene
