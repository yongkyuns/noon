# Source-equivalent ManimCE v0.21.0 growing-animation parity demo.
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()

left = Circle(
    radius=0.6,
    fill_color=BLUE,
    fill_opacity=0.8,
    stroke_color=WHITE,
    stroke_width=4,
).shift(3.0 * LEFT)
center = Square(
    side_length=1.2,
    fill_color=PINK,
    fill_opacity=0.8,
    stroke_color=WHITE,
    stroke_width=4,
)
right = Rectangle(
    width=1.8,
    height=1.0,
    fill_color=GREEN,
    fill_opacity=0.8,
    stroke_color=WHITE,
    stroke_width=4,
).shift(3.0 * RIGHT)

scene.play(
    LaggedStart(
        GrowFromPoint(left, 4.0 * LEFT + 2.0 * DOWN, point_color=YELLOW),
        GrowFromCenter(center),
        GrowFromEdge(right, DOWN),
        lag_ratio=0.25,
    ),
    run_time=2.5,
)

result = scene
