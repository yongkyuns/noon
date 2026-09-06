"""Paired Rust/Python ordered callback proof for analytic Line matching."""
from noon import Circle, Line, RED, Scene


class LiveLineMatchCallback(Scene):
    def construct(self):
        left = Circle(0.08).move_to((-0.5, 0.0, 0.0))
        right = Circle(0.08).move_to((0.5, 0.0, 0.0))
        line = Line((-0.5, 0.0, 0.0), (0.5, 0.0, 0.0)).set_color(RED)
        self.add(left, right, line)
        left.add_updater(lambda dot: dot.set_x(2.0))
        line.add_updater(
            lambda current: current.match_points(Line(left.get_center(), right.get_center()))
        )
        self.live_execution()
