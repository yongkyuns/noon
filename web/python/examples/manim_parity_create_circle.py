# Source-equivalent ManimCE v0.21.0 quickstart parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/tutorials/quickstart.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
circle = Circle()
circle.set_fill(PINK, opacity=0.5)
scene.play(Create(circle))

result = scene
