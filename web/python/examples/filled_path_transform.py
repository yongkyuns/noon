import math

from noon import Color, Path, Scene, Transform, VectorPath
from noon_layout import DOWN, LEFT, RIGHT, UP, Vec2

scene = Scene()


def rounded_loop(radius: float) -> VectorPath:
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


def five_point_star(outer_radius: float, inner_radius: float) -> VectorPath:
    points = []
    for index in range(10):
        angle = math.pi / 2.0 - index * math.pi / 5.0
        radius = outer_radius if index % 2 == 0 else inner_radius
        points.append(Vec2(math.cos(angle) * radius, math.sin(angle) * radius))
    path = VectorPath().move_to(points[0])
    for point in points[1:]:
        path.line_to(point)
    return path.close()


source = rounded_loop(1.35)
target = five_point_star(1.7, 0.7)
OUTLINE = Color(0.96, 0.96, 1.0)

shape = scene.path(
    source,
    fill=Color(0.18, 0.62, 0.96),
    stroke=OUTLINE,
    stroke_width=0.08,
    key="filled-transform",
)
target_shape = Path(
    target,
    fill=Color(0.78, 0.32, 0.94),
    stroke=OUTLINE,
    stroke_width=0.08,
)
scene.play(
    Transform(shape, target_shape, key="filled-transform.star"),
    duration=3.2,
    start_time=0.35,
    easing="ease_in_out_cubic",
)

result = scene
