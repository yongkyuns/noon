# Source-equivalent ManimCE v0.21.0 growing-animation parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/reference/manim.animation.growing.SpinInFromNothing.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
rectangle = Rectangle(
    width=2.2,
    height=1.0,
    fill_color=BLUE,
    fill_opacity=0.8,
    stroke_opacity=0.0,
).rotate(PI / 7).shift(1.5 * RIGHT + 0.5 * UP)
scene.play(SpinInFromNothing(rectangle))
result = scene
