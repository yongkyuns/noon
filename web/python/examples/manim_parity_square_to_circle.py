# Source-equivalent ManimCE v0.21.0 quickstart parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/tutorials/quickstart.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
circle = Circle()
circle.set_fill(PINK, opacity=0.5)

square = Square()
square.rotate(PI / 4)

scene.play(Create(square))
scene.play(Transform(square, circle))
scene.play(FadeOut(square))

result = scene
