# Source-equivalent ManimCE v0.21.0 composition parity demo.
# Output-affecting scene code intentionally matches the canonical parity fixture.

from noon import *

scene = Scene()

left = Circle(
    radius=0.35,
    fill_color=BLUE,
    fill_opacity=1.0,
    stroke_opacity=0.0,
).shift(2 * LEFT)
right = Circle(
    radius=0.35,
    fill_color=GREEN,
    fill_opacity=1.0,
    stroke_opacity=0.0,
).shift(2 * RIGHT)
scene.play(Succession(Wait(0.4), Add(left), Wait(0.6), Add(right)))

mapped = VGroup(
    Square(
        side_length=0.7,
        fill_color=PINK,
        fill_opacity=1.0,
        stroke_opacity=0.0,
    ).shift(0.6 * LEFT + 1.5 * DOWN),
    Square(
        side_length=0.7,
        fill_color=YELLOW,
        fill_opacity=1.0,
        stroke_opacity=0.0,
    ).shift(0.6 * RIGHT + 1.5 * DOWN),
)
scene.play(
    LaggedStartMap(
        FadeIn,
        mapped,
        run_time=2.2,
        lag_ratio=0.1,
        rate_func=linear,
    )
)

result = scene
