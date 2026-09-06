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

        circle.set_fill(blue, opacity=0.75)
        assert abs(circle.get_fill_opacity() - 0.75) < 1e-9
        assert abs(circle.get_stroke_opacity() - 1.0) < 1e-9
        circle.set_stroke(white, opacity=0.4)
        assert abs(circle.get_fill_opacity() - 0.75) < 1e-9
        assert abs(circle.get_stroke_opacity() - 0.4) < 1e-9
        circle.set_opacity(0.2)
        assert abs(circle.get_fill_opacity() - 0.2) < 1e-9
        assert abs(circle.get_stroke_opacity() - 0.2) < 1e-9

        independent_fill = Color(1.0, 0.25, 0.75)
        independent_stroke = Color(0.1, 0.8, 0.2)
        self.play(
            circle.animate.set_fill(independent_fill, opacity=0.8).set_stroke(
                independent_stroke,
                opacity=0.3,
            ),
            run_time=0.4,
            rate_func=linear,
        )
        assert abs(circle.get_fill_opacity() - 0.8) < 1e-9
        assert abs(circle.get_stroke_opacity() - 0.3) < 1e-9
        assert self.time == 0.4

        circle.set_fill(blue, opacity=1.0)
        circle.set_stroke(white, opacity=1.0)
        self.play(
            circle.animate.set_color(red).set_opacity(0.5),
            run_time=2.0,
            rate_func=linear,
        )
        assert self.time == 2.4

        circle.set_color(yellow)
        circle.set_opacity(1.0)
        assert self.time == 2.4
