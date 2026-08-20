import math

from noon import Color, Scene, VectorPath

scene = Scene()

# Mixed geometry scene: analytic primitives and cached vector meshes share one
# timeline and one persistent runtime.
backdrop = scene.rectangle(
    5.8,
    3.2,
    key="backdrop",
    fill=Color(0.08, 0.10, 0.16),
    stroke=Color(0.24, 0.30, 0.45),
    stroke_width=0.035,
    opacity=0.95,
)
scene.animate_opacity(
    backdrop,
    0.72,
    0.98,
    duration=2.4,
    easing="ease_in_out_cubic",
    key="backdrop.opacity",
)

# Four corner anchors show analytic circles and independent position tracks.
for index, (x, y) in enumerate(((-2.2, 1.05), (2.2, 1.05), (2.2, -1.05), (-2.2, -1.05))):
    anchor = scene.circle(
        0.23,
        key=f"anchor.{index}",
        position=(x, y),
        fill=Color(0.24, 0.78, 1.0),
        stroke=Color(0.88, 0.96, 1.0),
        stroke_width=0.03,
    )
    scene.animate_position(
        anchor,
        (x, y),
        (-x * 0.88, -y * 0.88),
        start_time=index * 0.15,
        duration=2.65,
        easing="ease_in_out_cubic",
        key=f"anchor.{index}.position",
    )

# A rounded diamond-like vector path built from cubic commands.
diamond = (
    VectorPath()
    .move_to((0.0, 0.92))
    .cubic_to((0.18, 0.92), (0.92, 0.18), (0.92, 0.0))
    .cubic_to((0.92, -0.18), (0.18, -0.92), (0.0, -0.92))
    .cubic_to((-0.18, -0.92), (-0.92, -0.18), (-0.92, 0.0))
    .cubic_to((-0.92, 0.18), (-0.18, 0.92), (0.0, 0.92))
    .close()
)
core = scene.path(
    diamond,
    key="core",
    fill=Color(0.64, 0.43, 1.0, 0.84),
    stroke=Color(0.92, 0.88, 1.0),
    stroke_width=0.055,
)
scene.animate_rotation(
    core,
    -0.25,
    math.tau - 0.25,
    duration=4.0,
    easing="ease_in_out_cubic",
    key="core.rotation",
)

# Three analytic bars move through the vector shape without retessellating it.
bar_colors = (
    Color(1.0, 0.42, 0.56),
    Color(1.0, 0.72, 0.24),
    Color(0.28, 0.90, 0.66),
)
for index, y in enumerate((-0.58, 0.0, 0.58)):
    bar = scene.rectangle(
        1.15,
        0.17,
        key=f"bar.{index}",
        position=(-1.85, y),
        fill=bar_colors[index],
        stroke=None,
        opacity=0.88,
    )
    scene.animate_position(
        bar,
        (-1.85, y),
        (1.85, -y),
        start_time=index * 0.28,
        duration=2.95,
        easing="ease_in_out_cubic",
        key=f"bar.{index}.position",
    )

result = scene
