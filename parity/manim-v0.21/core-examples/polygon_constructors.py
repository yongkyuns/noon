from manim import *


class PolygonConstructors(Scene):
    def construct(self):
        shapes = [
            Polygon([-1, -0.5, 0], [1, -0.5, 0], [0, 1, 0]).shift([-4, 1.5, 0]),
            RegularPolygon().shift([-1.5, 1.5, 0]),
            RegularPolygon(5).shift([1, 1.5, 0]),
            Star().shift([3.5, 1.5, 0]),
            RegularPolygram(6, density=2).shift([-2, -1.5, 0]),
            Star(7, inner_radius=0.6, density=3, start_angle=0.3).shift([2, -1.5, 0]),
        ]
        for shape, count in zip(shapes, [3, 6, 5, 10, 6, 14]):
            assert len(shape.get_vertices()) == count
            shape.set_fill(opacity=0.2)
        self.add(*shapes)
        self.wait(0.2)
