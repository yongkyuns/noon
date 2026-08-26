# Source-equivalent ManimCE v0.21.0 ScaleInPlace parity candidate.
# Canonical fixture: scale-in-place-square
# The upstream reference example uses Text, which is outside Noon's current exact
# text subset. This geometry scene is ordinary Manim source and intentionally matches
# the output-affecting code in the canonical geometry fixture.

from noon import *

scene = Scene()
square = Square(
    side_length=1.5,
    fill_color=BLUE,
    fill_opacity=1.0,
    stroke_opacity=0.0,
).shift(LEFT * 1.25)
scene.play(ScaleInPlace(square, 1.75))
result = scene
