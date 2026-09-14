# Source-equivalent ManimCE v0.21.0 CyclicReplace parity candidate.
# Paired with Rust `example_scenes::cyclic_replace`.
from noon import *


class CyclicReplaceThree(Scene):
    def construct(self):
        family = VGroup(
            Circle(radius=0.35).move_to(LEFT * 2),
            Circle(radius=0.35),
            Circle(radius=0.35).move_to(RIGHT * 2),
        )
        self.add(family)
        self.play(family.animate.shift(UP), run_time=0.25, rate_func=linear)
        self.play(CyclicReplace(family, run_time=1.0, rate_func=linear))
        expected = [ORIGIN + UP, RIGHT * 2 + UP, LEFT * 2 + UP]
        for member, point in zip(family, expected):
            center = member.get_center()
            assert abs(center[0] - point[0]) < 1e-6
            assert abs(center[1] - point[1]) < 1e-6
