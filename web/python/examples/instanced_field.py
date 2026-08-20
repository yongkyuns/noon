import math

from noon import Color, Scene

scene = Scene()

columns = 18
rows = 10
spacing_x = 0.34
spacing_y = 0.34

# A dense analytic circle field. Every dot has the same geometry, while color
# and transform vary per instance. The renderer should batch this into a tiny
# number of draw calls and only upload dirty instance ranges as tracks advance.
for row in range(rows):
    for column in range(columns):
        index = row * columns + column
        x = (column - (columns - 1) / 2.0) * spacing_x
        y = ((rows - 1) / 2.0 - row) * spacing_y
        radius_from_center = math.sqrt(x * x + y * y)
        phase = radius_from_center * 1.35 + (column % 3) * 0.16
        angle = math.atan2(y, x) + 0.72
        target_radius = radius_from_center * (0.72 + 0.10 * math.sin(phase))
        tx = math.cos(angle) * target_radius
        ty = math.sin(angle) * target_radius

        color_mix = (row + column) / (rows + columns - 2)
        color = Color(
            0.32 + 0.58 * color_mix,
            0.78 - 0.30 * color_mix,
            1.00 - 0.22 * color_mix,
            0.94,
        )
        dot = scene.circle(
            0.105,
            key=f"dot.{index}",
            position=(x, y),
            fill=color,
            stroke=None,
        )
        scene.animate_position(
            dot,
            (x, y),
            (tx, ty),
            start_time=(index % 18) * 0.018,
            duration=3.15,
            easing="ease_in_out_cubic",
            key=f"dot.{index}.position",
        )
        scene.animate_opacity(
            dot,
            0.34 + 0.52 * ((index % 7) / 6.0),
            1.0,
            start_time=(index % 11) * 0.022,
            duration=1.25,
            easing="ease_in_out_cubic",
            key=f"dot.{index}.opacity",
        )

result = scene
