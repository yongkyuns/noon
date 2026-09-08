"""Paired with Rust scale_in_place: capture the target after the earlier play."""
from noon import Color, Scene, ScaleInPlace, Square, linear


class OrdinaryScaleInPlace(Scene):
    def construct(self):
        square = Square(side_length=1.0, fill_color=Color(0, 0, 1),
                        fill_opacity=1.0, stroke_opacity=0.0).shift((-0.5, 0.25))
        self.add(square)
        deferred = ScaleInPlace(square, 2.0, run_time=0.75, rate_func=linear)
        self.play(square.animate.shift((1.5, -0.25)), run_time=0.25, rate_func=linear)
        self.play(deferred)
        assert tuple(square.get_center()) == (1.0, 0.0)
        assert abs(square.width - 2.0) < 1e-6
