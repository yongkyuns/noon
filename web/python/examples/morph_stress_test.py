import math

from noon import Color, Scene, Transform, VectorPath

scene = Scene()

# A deliberately dense morph scene that exercises the production path:
# Python builds semantic geometry once, Noon prepares 12 reusable fixed-topology
# morph meshes, then 600 objects animate by updating instance data only.
#
# In the browser metrics this should settle around:
#   - 600 objects
#   - ~12 path draw calls (one per reusable morph mesh)
#   - no path-geometry upload during steady morph playback
columns = 30
rows = 20
variant_count = 12
spacing_x = 0.20
spacing_y = 0.20
object_count = columns * rows


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
    # entries. Each target remains a symmetric five-point star so visual defects
    # in correspondence or stroke joins are easy to spot under load.
    phase = (variant / variant_count) * math.pi * 0.36
    outer = 0.088 + 0.006 * math.sin(variant * 1.7)
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


source = rounded_source(0.074)
targets = [star_target(variant) for variant in range(variant_count)]

for row in range(rows):
    for column in range(columns):
        index = row * columns + column
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
            stroke_width=0.018,
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

assert object_count == 600
result = scene
