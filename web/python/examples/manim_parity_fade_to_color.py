# Source-equivalent ManimCE v0.21.0 FadeToColor parity candidate.
# The upstream reference example uses Text, which is outside Noon's current exact
# text subset. This geometry scene exercises the same FadeToColor animation API
# without substituting a Noon-only animation path.

from noon import *

scene = Scene()
square = Square(
    side_length=1.5,
    fill_color=BLUE,
    fill_opacity=0.35,
    stroke_color=GREEN,
    stroke_opacity=0.65,
    stroke_width=6,
).shift(1.25 * RIGHT)
scene.play(FadeToColor(square, RED))
result = scene
