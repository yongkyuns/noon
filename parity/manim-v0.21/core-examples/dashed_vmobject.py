from manim import *


class DashedVMobjectExample(Scene):
    def construct(self):
        closed_source = Circle(radius=1.25, color=BLUE).shift(LEFT * 2.5)
        closed = DashedVMobject(
            closed_source,
            num_dashes=10,
            dashed_ratio=0.55,
            dash_offset=0.2,
        )

        equal_source = VMobject(color=YELLOW)
        equal_source.set_points_as_corners(
            [
                [-1.25, -0.75, 0.0],
                [-0.75, -0.75, 0.0],
                [1.25, 0.75, 0.0],
            ]
        )
        equal_source.shift(RIGHT * 2.5 + UP * 0.8)
        equal = DashedVMobject(
            equal_source,
            num_dashes=6,
            dashed_ratio=0.5,
            equal_lengths=True,
        )

        direct_source = VMobject(color=GREEN)
        direct_source.set_points_as_corners(
            [
                [-1.25, -0.75, 0.0],
                [-0.75, -0.75, 0.0],
                [1.25, 0.75, 0.0],
            ]
        )
        direct_source.shift(RIGHT * 2.5 + DOWN * 0.8)
        direct = DashedVMobject(
            direct_source,
            num_dashes=6,
            dashed_ratio=0.5,
            equal_lengths=False,
        )

        self.add(closed, equal, direct)
        self.wait(0.2)
