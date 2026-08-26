# Source-equivalent ManimCE v0.21.0 DrawBorderThenFill parity demo.
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
scene.play(DrawBorderThenFill(Square(fill_opacity=1, fill_color=ORANGE)))

result = scene
