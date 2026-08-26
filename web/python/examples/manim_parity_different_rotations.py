# Source-equivalent ManimCE v0.21.0 quickstart parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/tutorials/quickstart.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
left_square = Square(color=BLUE, fill_opacity=0.7).shift(2 * LEFT)
right_square = Square(color=GREEN, fill_opacity=0.7).shift(2 * RIGHT)
scene.play(
    left_square.animate.rotate(PI),
    Rotate(right_square, angle=PI),
    run_time=2,
)
scene.wait()

result = scene
