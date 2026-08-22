import math

from noon import (
    BLUE,
    GREEN,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    YELLOW,
    Circle,
    Scene,
    VGroup,
    Vec2,
)

scene = Scene()

ROWS = 10
COLUMNS = 18
PALETTE = (BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE)

# One analytic geometry repeated across a semantic grid. The animation only
# changes position so batching/dirty instance behavior remains the focus.
dots = [Circle(0.105, color=PALETTE[index % len(PALETTE)]).set_stroke(None) for index in range(ROWS * COLUMNS)]
VGroup(*dots).arrange_in_grid(rows=ROWS, cols=COLUMNS, buff=(0.13, 0.13))

for index, dot in enumerate(dots):
    scene.add(dot, key=f"dot.{index}")
    position = dot.get_center()
    column = index % COLUMNS
    phase = position.length() * 1.35 + (column % 3) * 0.16
    angle = math.atan2(position.y, position.x) + 0.72
    radius = position.length() * (0.72 + 0.10 * math.sin(phase))
    target = Vec2(math.cos(angle) * radius, math.sin(angle) * radius)
    scene.animate_position(
        dot,
        position,
        target,
        duration=3.0,
        easing="ease_in_out_cubic",
        key=f"dot.{index}.position",
    )

result = scene
