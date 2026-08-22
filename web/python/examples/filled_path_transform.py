import math

from noon import (
    BLUE,
    PURPLE,
    WHITE,
    DOWN,
    LEFT,
    RIGHT,
    UP,
    Path,
    Scene,
    Transform,
    Vec2,
    VectorPath,
)

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


shape = scene.path(
    rounded_loop(1.35),
    fill=BLUE,
    stroke=WHITE,
    stroke_width=0.08,
    key="filled-transform",
)
target = Path(
    five_point_star(1.7, 0.7),
    fill=PURPLE,
    stroke=WHITE,
    stroke_width=0.08,
)
scene.play(
    Transform(shape, target, key="filled-transform.star"),
    run_time=3.2,
    easing="ease_in_out_cubic",
)

result = scene
