"""Paired with native morph_stress; context.object_count selects 12–10,000 paths."""
import math
from noon import (BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE,
                  DOWN, LEFT, RIGHT, UP, Path, Scene, Transform, Vec2, VectorPath)

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


def five_point_star(outer_radius: float, inner_radius: float, phase: float = 0) -> VectorPath:
    points = []
    for index in range(10):
        angle = phase + math.pi / 2.0 - index * math.pi / 5.0
        radius = outer_radius if index % 2 == 0 else inner_radius
        points.append(Vec2(math.cos(angle) * radius, math.sin(angle) * radius))
    path = VectorPath().move_to(points[0])
    for point in points[1:]:
        path.line_to(point)
    return path.close()


class MorphStress(Scene):
    def construct(self):
        count = globals().get("context", {}).get("object_count", 96)
        if isinstance(count, bool) or not isinstance(count, int) or not 12 <= count <= 10000:
            raise ValueError("morph object count must be between 12 and 10000")
        columns = math.ceil(math.sqrt(count * 1.5))
        rows = math.ceil(count / columns)
        dx, dy = 5.8 / max(columns - 1, 1), 3.8 / max(rows - 1, 1)
        radius = min(dx, dy) * 0.37
        width = max(radius * 0.24, 0.0025)
        colors = (BLUE, TEAL, GREEN, YELLOW, ORANGE, RED, PINK, PURPLE)
        source = rounded_loop(radius)
        targets = []
        for variant in range(12):
            outer = radius * (1.18 + 0.08 * math.sin(variant * 1.7))
            inner = outer * (0.42 + 0.05 * math.cos(variant * 0.9))
            targets.append(five_point_star(outer, inner, variant / 12 * math.pi * 0.36))
        animations = []
        for index in range(count):
            paint = dict(fill=None, stroke=colors[index % len(colors)], stroke_width=width * 100)
            position = (-2.9 + index % columns * dx, 1.9 - index // columns * dy)
            shape = Path(source, **paint).shift(position)
            target = Path(targets[index % 12], **paint).shift(position)
            self.add(shape)
            animations.append(Transform(shape, target))
        self.play(*animations, run_time=3.4, easing="ease_in_out_cubic")
