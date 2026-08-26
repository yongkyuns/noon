# Source-equivalent ManimCE v0.21.0 MoveToTarget parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/reference/manim.animation.transform.MoveToTarget.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
circle = Circle()
circle.generate_target()
circle.target.set_fill(color=GREEN, opacity=0.5)
circle.target.shift(2 * RIGHT + UP).scale(0.5)
scene.add(circle)
scene.play(MoveToTarget(circle))

result = scene
