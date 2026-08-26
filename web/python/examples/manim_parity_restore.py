# Source-equivalent ManimCE v0.21.0 Restore parity candidate.
# This geometry scene exercises the same save_state -> mutate -> Restore contract as
# Manim's public Restore example without depending on the still-open exact Text surface.

from noon import *

scene = Scene()
square = Square(
    side_length=1.5,
    fill_color=BLUE,
    fill_opacity=0.4,
    stroke_color=GREEN,
    stroke_opacity=0.6,
    stroke_width=5,
).shift(0.5 * RIGHT)
square.save_state()
square.shift(2 * RIGHT).scale(1.75).rotate(0.3).set_color(PURPLE)
scene.add(square)
scene.play(Restore(square), run_time=2)
result = scene
