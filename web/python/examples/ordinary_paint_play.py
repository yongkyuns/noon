"""Ordinary fill/stroke color and paint-opacity animation over shared Rust."""

from noon import Circle, Color, Scene, linear


class OrdinaryPaintPlay(Scene):
    def construct(self):
        blue = Color(0.0, 0.0, 1.0)
        white = Color(1.0, 1.0, 1.0)
        red = Color(1.0, 0.0, 0.0)
        yellow = Color(1.0, 1.0, 0.0)
        circle = Circle(
            radius=0.4,
            fill_color=blue,
            fill_opacity=1.0,
            stroke_color=white,
            stroke_opacity=1.0,
        )
        self.add(circle)

        self.play(
            circle.animate.set_color(red).set_opacity(0.5),
            run_time=2.0,
            rate_func=linear,
        )
        assert self.time == 2.0

        circle.set_color(yellow)
        circle.set_opacity(1.0)
        assert self.time == 2.0
