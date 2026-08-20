import math

from noon import Color, Scene, Transform, VectorPath

scene = Scene()

# A deliberately dense morph scene that exercises the production path. The
# playground passes object_count through the worker context so the same source
# can benchmark several scales without changing the animation semantics.
authoring_context = globals().get("context", {})
requested_count = authoring_context.get("object_count", 600) if isinstance(authoring_context, dict) else 600
if isinstance(requested_count, bool) or not isinstance(requested_count, int):
    raise TypeError("object_count must be an integer")
if requested_count <= 0 or requested_count > 10_000:
    raise ValueError("object_count must be between 1 and 10000")

object_count = requested_count
variant_count = 12
aspect = 1.5
columns = math.ceil(math.sqrt(object_count * aspect))
rows = math.ceil(object_count / columns)
spacing_x = 5.8 / max(columns - 1, 1)
spacing_y = 3.8 / max(rows - 1, 1)
source_radius = min(spacing_x, spacing_y) * 0.37
stroke_width = max(source_radius * 0.24, 0.0025)


def rounded_source(radius: float) -> VectorPath:
    # Four cubic Beziers approximating a circle. Using the same source shape for
    # every variant lets target shape diversity, rather than source complexity,
    # determine the number of cached morph meshes.
    k = radius * 0.58
    return (
        VectorPath()
        .move_to((0.0, radius))
        .cubic_to((k, radius), (radius, k), (radius, 0.0))
        .cubic_to((radius, -k), (k, -radius), (0.0, -radius))
        .cubic_to((-k, -radius), (-radius, -k), (-radius, 0.0))
        .cubic_to((-radius, k), (-k, radius), (0.0, radius))
        .close()
    )


def star_target(variant: int) -> VectorPath:
    # Twelve subtly different targets create twelve reusable geometry-cache
    # entries. Every object therefore updates only its compact instance record
    # during steady-state morph playback.
    phase = (variant / variant_count) * math.pi * 0.36
    outer = source_radius * (1.18 + 0.08 * math.sin(variant * 1.7))
    inner = outer * (0.42 + 0.05 * math.cos(variant * 0.9))
    points = []
    for point_index in range(10):
        angle = phase + math.pi / 2.0 + point_index * math.pi / 5.0
        radius = outer if point_index % 2 == 0 else inner
        points.append((math.cos(angle) * radius, math.sin(angle) * radius))

    target = VectorPath().move_to(points[0])
    for point in points[1:]:
        target.line_to(point)
    return target.close()


source = rounded_source(source_radius)
targets = [star_target(variant) for variant in range(variant_count)]

for index in range(object_count):
    row = index // columns
    column = index % columns
    variant = index % variant_count
    x = (column - (columns - 1) / 2.0) * spacing_x
    y = ((rows - 1) / 2.0 - row) * spacing_y
    phase = (row * 0.19 + column * 0.13) % (2.0 * math.pi)

    color_t = variant / (variant_count - 1)
    color = Color(
        0.34 + 0.58 * color_t,
        0.80 - 0.34 * color_t,
        0.98 - 0.18 * math.sin(variant * 0.7) ** 2,
        0.92,
    )
    shape = scene.path(
        source,
        position=(x, y),
        rotation=phase * 0.18,
        fill=None,
        stroke=color,
        stroke_width=stroke_width,
        key=f"stress.{index}",
    )
    scene.play(
        Transform(shape, targets[variant], key=f"stress.{index}.morph"),
        duration=4.0,
        easing="ease_in_out_cubic",
    )
    scene.animate_rotation(
        shape,
        phase * 0.18,
        phase * 0.18 + (0.75 if (row + column) % 2 == 0 else -0.75),
        duration=4.0,
        easing="ease_in_out_cubic",
        key=f"stress.{index}.rotation",
    )

result = scene
