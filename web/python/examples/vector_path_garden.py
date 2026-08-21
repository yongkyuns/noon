import math

from noon import Color, Scene, VectorPath

scene = Scene()

# One semantic cubic path is reused for every petal. Equal paths should share
# tessellated geometry in the renderer while each instance keeps its own style
# and transform.
petal = (
    VectorPath()
    .move_to((0.0, 0.0))
    .cubic_to((0.12, 0.58), (0.46, 0.82), (0.0, 1.18))
    .cubic_to((-0.46, 0.82), (-0.12, 0.58), (0.0, 0.0))
    .close()
)

palette = [
    Color(0.63, 0.45, 1.00),
    Color(0.28, 0.76, 1.00),
    Color(0.28, 0.90, 0.67),
    Color(1.00, 0.72, 0.28),
    Color(1.00, 0.42, 0.55),
    Color(0.92, 0.45, 0.92),
]

for index in range(12):
    angle = index * math.tau / 12.0
    radius = 0.88
    obj = scene.path(
        petal,
        key=f"petal.{index}",
        position=(math.cos(angle) * radius, math.sin(angle) * radius),
        rotation=angle - math.pi / 2.0,
        scale=(0.78, 0.78),
        fill=palette[index % len(palette)],
        stroke=Color(1.0, 1.0, 1.0, 0.72),
        stroke_width=0.025,
        opacity=0.92,
    )
    scene.animate_rotation(
        obj,
        angle - math.pi / 2.0,
        angle + math.tau - math.pi / 2.0,
        duration=4.0,
        easing="ease_in_out_cubic",
        key=f"petal.{index}.rotation",
    )
    scene.animate_opacity(
        obj,
        0.36 if index % 2 else 0.95,
        0.95 if index % 2 else 0.36,
        start_time=(index % 4) * 0.12,
        duration=2.7,
        easing="ease_in_out_cubic",
        key=f"petal.{index}.opacity",
    )

# A second, more complex path exercises quadratic + cubic commands.
leaf = (
    VectorPath()
    .move_to((-0.45, -0.15))
    .quadratic_to((0.0, 0.72), (0.45, -0.15))
    .cubic_to((0.20, 0.10), (-0.20, 0.10), (-0.45, -0.15))
    .close()
)
for index in range(6):
    angle = index * math.tau / 6.0 + math.pi / 6.0
    obj = scene.path(
        leaf,
        key=f"leaf.{index}",
        position=(math.cos(angle) * 2.35, math.sin(angle) * 1.30),
        rotation=angle,
        scale=(0.82, 0.82),
        fill=Color(0.24, 0.82, 0.63, 0.82),
        stroke=Color(0.82, 1.0, 0.93, 0.74),
        stroke_width=0.035,
    )
    scene.animate_position(
        obj,
        (math.cos(angle) * 2.35, math.sin(angle) * 1.30),
        (math.cos(angle + math.pi) * 2.35, math.sin(angle + math.pi) * 1.30),
        duration=4.0,
        easing="ease_in_out_cubic",
        key=f"leaf.{index}.position",
    )

center = scene.circle(
    0.48,
    key="center",
    fill=Color(1.0, 0.72, 0.24),
    stroke=Color(1.0, 0.94, 0.72),
    stroke_width=0.04,
)
scene.animate_rotation(center, 0.0, math.tau, duration=4.0, key="center.rotation")

result = scene
