# Source-equivalent ManimCE v0.21.0 quickstart parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/tutorials/quickstart.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
circle = Circle()
circle.set_fill(PINK, opacity=0.5)

square = Square()
square.set_fill(BLUE, opacity=0.5)

square.next_to(circle, RIGHT, buff=0.5)
scene.play(Create(circle), Create(square))

result = scene
