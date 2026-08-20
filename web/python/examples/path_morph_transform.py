from noon import Color, Scene, Transform, VectorPath

scene = Scene()

# A rounded closed loop morphs into a sharp star. Noon plans source/target
# correspondence once, then WebGPU interpolates the fixed-topology stroke mesh
# from the independent morph progress channel.
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
    fill=None,
    stroke=Color(0.72, 0.48, 1.0),
    stroke_width=0.14,
    key="morph-shape",
)

scene.play(
    Transform(shape, target, key="morph-shape.transform"),
    duration=4.0,
    easing="ease_in_out_cubic",
)

# A slow rotation makes the changing silhouette easy to distinguish from a
# simple static redraw while remaining independent from geometric morphing.
scene.animate_rotation(
    shape,
    0.0,
    0.55,
    duration=4.0,
    easing="ease_in_out_cubic",
    key="morph-shape.rotation",
)

result = scene
