# Source-equivalent ManimCE v0.21.0 FocusOn parity demo.
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
scene.play(FocusOn(2 * RIGHT))

result = scene
