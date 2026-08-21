import math

from noon import Color, Scene

scene = Scene()

colors = [
    Color(0.55, 0.43, 1.00),
    Color(0.20, 0.78, 0.98),
    Color(0.23, 0.88, 0.63),
    Color(1.00, 0.69, 0.25),
    Color(0.98, 0.38, 0.49),
]

# A staggered choreography of analytic primitives. Each object owns independent
# position/rotation/opacity tracks, but the Rust runtime evaluates one timeline.
for row in range(3):
    for column in range(5):
        index = row * 5 + column
        x0 = -2.65 + column * 1.32
        y0 = 1.20 - row * 1.20
        delay = index * 0.09
        color = colors[(row + column) % len(colors)]

        if (row + column) % 2 == 0:
            obj = scene.circle(
                0.24 + 0.025 * row,
                key=f"dot.{index}",
                position=(x0, y0),
                fill=color,
                stroke=Color(1.0, 1.0, 1.0, 0.72),
                stroke_width=0.025,
            )
        else:
            obj = scene.rectangle(
                0.52,
                0.52,
                key=f"tile.{index}",
                position=(x0, y0),
                rotation=-0.35,
                fill=color,
                stroke=Color(1.0, 1.0, 1.0, 0.70),
                stroke_width=0.025,
            )
            scene.animate_rotation(
                obj,
                -0.35,
                math.tau - 0.35,
                start_time=delay,
                duration=2.15,
                easing="ease_in_out_cubic",
                key=f"tile.{index}.rotation",
            )

        scene.animate_position(
            obj,
            (x0, y0),
            (-x0 * 0.84, -y0 * 0.72),
            start_time=delay,
            duration=2.20,
            easing="ease_in_out_cubic",
            key=f"shape.{index}.position",
        )
        scene.animate_opacity(
            obj,
            0.28,
            1.0,
            start_time=delay,
            duration=0.72,
            easing="ease_in_out_cubic",
            key=f"shape.{index}.opacity",
        )

# Two long analytic lines cross behind the choreography and rotate continuously.
for index, angle in enumerate((0.0, math.pi / 2.0)):
    line = scene.line(
        (-3.2, 0.0),
        (3.2, 0.0),
        key=f"guide.{index}",
        rotation=angle,
        stroke=Color(0.48, 0.55, 0.78, 0.30),
        stroke_width=0.025,
        opacity=0.55,
    )
    scene.animate_rotation(
        line,
        angle,
        angle + math.pi,
        duration=4.0,
        easing="ease_in_out_cubic",
        key=f"guide.{index}.rotation",
    )

result = scene
