import math

from noon import Color, Scene

scene = Scene()
circle = scene.circle(
    0.65,
    fill=Color(0.98, 0.38, 0.36),
    stroke=Color(1.0, 1.0, 1.0),
    stroke_width=0.04,
)
rectangle = scene.rectangle(
    1.5,
    0.9,
    rotation=-0.7,
    fill=Color(0.27, 0.65, 0.96),
    stroke=Color(1.0, 1.0, 1.0),
    stroke_width=0.04,
)
line = scene.line(
    (-1.2, 0.0),
    (1.2, 0.0),
    position=(0.0, -1.55),
    rotation=-0.35,
    stroke=Color(0.30, 0.88, 0.57),
    stroke_width=0.10,
)

timing = {"duration": 4.0, "easing": "ease_in_out_cubic"}
scene.animate_position(circle, (-2.1, 0.8), (2.1, -0.8), **timing)
scene.animate_position(rectangle, (2.1, 0.8), (-2.1, -0.8), **timing)
scene.animate_rotation(rectangle, -0.7, math.tau - 0.7, **timing)
scene.animate_rotation(line, -0.35, math.tau - 0.35, **timing)

result = scene
