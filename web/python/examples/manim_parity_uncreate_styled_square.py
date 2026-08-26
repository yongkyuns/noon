# Source-equivalent ManimCE v0.21.0 Uncreate parity demo.
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()

square = Square()
square.set_fill(PINK, opacity=0.35)
square.set_stroke(BLUE, width=8, opacity=0.65)
scene.add(square)
scene.play(Uncreate(square))

result = scene
