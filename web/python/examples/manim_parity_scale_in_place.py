# Source-equivalent ManimCE v0.21.0 ScaleInPlace parity candidate.
# The upstream reference example uses Text, which is outside Noon's current exact
# text subset. This geometry fixture exercises the same ScaleInPlace animation API
# without substituting a Noon-only animation path.

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
