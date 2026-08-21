from noon import Color, Path, Scene, Transform, VectorPath

scene = Scene()

source = (
    VectorPath()
    .move_to((0.0, 1.65))
    .cubic_to((0.95, 1.65), (1.65, 0.95), (1.65, 0.0))
    .cubic_to((1.65, -0.95), (0.95, -1.65), (0.0, -1.65))
    .cubic_to((-0.95, -1.65), (-1.65, -0.95), (-1.65, 0.0))
    .cubic_to((-1.65, 0.95), (-0.95, 1.65), (0.0, 1.65))
    .close()
)

target = (
    VectorPath()
    .move_to((0.0, 2.0))
    .line_to((0.47, 0.65))
    .line_to((1.9, 0.62))
    .line_to((0.76, -0.25))
    .line_to((1.18, -1.62))
    .line_to((0.0, -0.82))
    .line_to((-1.18, -1.62))
    .line_to((-0.76, -0.25))
    .line_to((-1.9, 0.62))
    .line_to((-0.47, 0.65))
    .close()
)

shape = scene.path(
    source,
    fill=Color(0.18, 0.62, 0.96),
    stroke=Color(0.96, 0.96, 1.0),
    stroke_width=0.08,
    key="filled-morph",
)
target_shape = Path(
    target,
    fill=Color(0.78, 0.32, 0.94),
    stroke=Color(0.96, 0.96, 1.0),
    stroke_width=0.08,
)

scene.play(
    Transform(shape, target_shape, key="filled-morph.transform"),
    duration=4.0,
    easing="ease_in_out_cubic",
)
result = scene
