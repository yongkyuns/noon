"""Source-equivalent deterministic Transform path-arc demo."""
from noon import *


class TransformPathArc(Scene):
    def construct(self):
        circle = Circle(radius=0.5).set_fill(BLUE, opacity=1).move_to((-2, 0, 0))
        target = circle.copy().move_to((2, 0, 0))
        self.add(circle)
        self.play(
            Transform(
                circle,
                target,
                path_arc=PI,
                run_time=2,
                rate_func=linear,
            )
        )
