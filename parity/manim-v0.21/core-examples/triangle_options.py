from manim import *


class TriangleOptions(Scene):
    def construct(self):
        shapes = [
            Triangle().shift([-3.5, 0, 0]),
            Triangle(radius=1.5, start_angle=0.3),
            Triangle(radius=0.7, start_angle=None).shift([3.5, 0, 0]),
        ]
        for shape in shapes:
            assert isinstance(shape, RegularPolygon)
            assert len(shape.get_vertices()) == 3
            assert len(shape.get_vertex_groups()) == 1
            shape.set_fill(opacity=0.2)
        self.add(*shapes)
        self.wait(0.2)
