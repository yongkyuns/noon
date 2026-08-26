# Source-equivalent ManimCE v0.21.0 ApplyMethod parity candidate.
# Both operations use ordinary Manim bound-method syntax and stay on Noon's shared
# deterministic target-state Transform path.

from noon import *

scene = Scene()
square = Square(
    side_length=1.4,
    fill_color=BLUE,
    fill_opacity=0.6,
    stroke_opacity=0.0,
).shift(1.5 * LEFT)
scene.play(ApplyMethod(square.shift, 3 * RIGHT))
scene.play(ApplyMethod(square.set_fill, RED, {"opacity": 0.25}))
result = scene
