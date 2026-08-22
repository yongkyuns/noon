import math

from noon import (
    BLUE,
    GREEN,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    YELLOW,
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Path,
    Scene,
    Transform,
    VGroup,
    Vec2,
    VectorPath,
)

scene = Scene()

# Dense by design: this is the one gallery entry whose purpose is scale. The
# picker uses one representative count; context can still drive larger profiling.
authoring_context = globals().get("context", {})
requested_count = (
    authoring_context.get("object_count", 600)
    if isinstance(authoring_context, dict)
    else 600
)
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
PALETTE = (BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE)


def rounded_source(radius: float) -> VectorPath:
    handle = radius * 0.58
    return (
        VectorPath()
        .move_to(UP * radius)
        .cubic_to(Vec2(handle, radius), Vec2(radius, handle), RIGHT * radius)
        .cubic_to(Vec2(radius, -handle), Vec2(handle, -radius), DOWN * radius)
        .cubic_to(Vec2(-handle, -radius), Vec2(-radius, -handle), LEFT * radius)
        .cubic_to(Vec2(-radius, handle), Vec2(-handle, radius), UP * radius)
        .close()
    )


def star_target(variant: int) -> VectorPath:
    # Twelve target variants create twelve reusable geometry-cache entries.
    phase = (variant / variant_count) * math.pi * 0.36
    outer = source_radius * (1.18 + 0.08 * math.sin(variant * 1.7))
    inner = outer * (0.42 + 0.05 * math.cos(variant * 0.9))
    points = []
    for point_index in range(10):
        angle = phase + math.pi / 2.0 + point_index * math.pi / 5.0
        radius = outer if point_index % 2 == 0 else inner
        points.append(Vec2(math.cos(angle) * radius, math.sin(angle) * radius))

    target = VectorPath().move_to(points[0])
    for point in points[1:]:
        target.line_to(point)
    return target.close()


source = rounded_source(source_radius)
targets = [star_target(variant) for variant in range(variant_count)]
shapes = [
    Path(
        source,
        fill=None,
        stroke=PALETTE[index % len(PALETTE)],
        stroke_width=stroke_width,
    )
    for index in range(object_count)
]

# The workload still derives its dimensions from object count/aspect, but object
# placement is delegated to the same bounds-aware group layout used by normal scenes.
VGroup(*shapes).arrange_in_grid(
    rows=rows,
    cols=columns,
    buff=(max(spacing_x - 2 * source_radius, 0.0), max(spacing_y - 2 * source_radius, 0.0)),
)
for index, shape in enumerate(shapes):
    scene.add(shape, key=f"stress.{index}")

scene.play(
    *(
        Transform(shape, targets[index % variant_count], key=f"stress.{index}.morph")
        for index, shape in enumerate(shapes)
    ),
    run_time=3.4,
    easing="ease_in_out_cubic",
)

result = scene
