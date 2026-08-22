import math

from noon import Color, Scene
from noon_layout import grid

scene = Scene()

columns = 18
rows = 10
if isinstance(columns, bool) or not isinstance(columns, int) or columns <= 0:
    raise ValueError("columns must be a positive integer")
if isinstance(rows, bool) or not isinstance(rows, int) or rows <= 0:
    raise ValueError("rows must be a positive integer")

# One grid, one analytic geometry, many instance records. This example is about
# batching and dirty instance uploads rather than introducing new animation APIs.
positions = grid(rows, columns, spacing=(0.34, 0.34))
color_denominator = max(rows + columns - 2, 1)

for index, position in enumerate(positions):
    row, column = divmod(index, columns)
    phase = position.length() * 1.35 + (column % 3) * 0.16
    target = position.rotated(0.72) * (0.72 + 0.10 * math.sin(phase))

    color_mix = (row + column) / color_denominator
    dot = scene.circle(
        0.105,
        key=f"dot.{index}",
        position=position,
        fill=Color(
            0.32 + 0.58 * color_mix,
            0.78 - 0.30 * color_mix,
            1.00 - 0.22 * color_mix,
            0.94,
        ),
        stroke=None,
    )
    scene.animate_position(
        dot,
        position,
        target,
        start_time=(column % 6) * 0.04,
        duration=3.0,
        easing="ease_in_out_cubic",
        key=f"dot.{index}.position",
    )
    scene.animate_opacity(
        dot,
        0.35 + 0.5 * ((index % 7) / 6.0),
        1.0,
        start_time=(index % 5) * 0.04,
        duration=1.2,
        easing="ease_in_out_cubic",
        key=f"dot.{index}.opacity",
    )

result = scene
