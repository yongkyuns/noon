# Source-equivalent ManimCE v0.21.0 Rotating parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/reference/manim.animation.rotation.Rotating.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
square = Square(
    side_length=1.5,
    fill_color=BLUE,
    fill_opacity=0.7,
    stroke_opacity=0.0,
)
scene.add(square)
scene.play(Rotating(square))

result = scene
