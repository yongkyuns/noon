# Source-equivalent ManimCE v0.21.0 ShowIncreasingSubsets parity demo.
# Upstream: https://docs.manim.community/en/v0.21.0/reference/manim.animation.creation.ShowIncreasingSubsets.html
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()
group = VGroup(
    Square(
        side_length=1.2,
        fill_color=RED,
        fill_opacity=0.35,
        stroke_color=WHITE,
        stroke_opacity=0.65,
        stroke_width=4,
    ).shift(1.2 * LEFT),
    Circle(
        radius=0.65,
        fill_color=BLUE,
        fill_opacity=0.2,
        stroke_color=WHITE,
        stroke_opacity=0.4,
        stroke_width=4,
    ).shift(1.2 * RIGHT),
)
scene.play(ShowIncreasingSubsets(group))

result = scene
