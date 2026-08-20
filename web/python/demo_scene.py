import math

from noon import Color, Scene, Transform, VectorPath

scene = Scene()
circle = scene.circle(
    0.65,
    key="circle",
    fill=Color(0.98, 0.38, 0.36),
    stroke=Color(1.0, 1.0, 1.0),
    stroke_width=0.04,
)
rectangle = scene.rectangle(
    1.5,
    0.9,
    key="rectangle",
    rotation=-0.7,
    fill=Color(0.27, 0.65, 0.96),
    stroke=Color(1.0, 1.0, 1.0),
    stroke_width=0.04,
)
line = scene.line(
    (-1.2, 0.0),
    (1.2, 0.0),
    key="line",
    position=(0.0, -1.55),
    rotation=-0.35,
    stroke=Color(0.30, 0.88, 0.57),
    stroke_width=0.10,
)
curve = scene.path(
    VectorPath()
    .move_to((-0.8, -0.2))
    .cubic_to((-0.8, 0.55), (0.0, 0.85), (0.0, 0.2))
    .cubic_to((0.0, 0.85), (0.8, 0.55), (0.8, -0.2))
    .cubic_to((0.65, -0.8), (-0.65, -0.8), (-0.8, -0.2))
    .close(),
    key="curve",
    position=(0.0, 1.45),
    scale=(0.75, 0.75),
    fill=None,
    stroke=Color(0.72, 0.48, 1.0),
    stroke_width=0.10,
    opacity=0.95,
)

timing = {"duration": 4.0, "easing": "ease_in_out_cubic"}
scene.animate_position(circle, (-2.1, 0.8), (2.1, -0.8), key="circle.position", **timing)
scene.animate_position(
    rectangle,
    (2.1, 0.8),
    (-2.1, -0.8),
    key="rectangle.position",
    **timing,
)
scene.animate_rotation(
    rectangle, -0.7, math.tau - 0.7, key="rectangle.rotation", **timing
)
scene.animate_rotation(line, -0.35, math.tau - 0.35, key="line.rotation", **timing)
morph_target = (
    VectorPath()
    .move_to((0.0, 0.9))
    .line_to((0.28, 0.28))
    .line_to((0.9, 0.2))
    .line_to((0.42, -0.2))
    .line_to((0.58, -0.82))
    .line_to((0.0, -0.48))
    .line_to((-0.58, -0.82))
    .line_to((-0.42, -0.2))
    .line_to((-0.9, 0.2))
    .line_to((-0.28, 0.28))
    .close()
)
scene.play(Transform(curve, morph_target, key="curve.transform"), **timing)

result = scene
