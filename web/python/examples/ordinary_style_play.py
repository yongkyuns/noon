"""Ordinary style animation and setters over one shared Rust session."""

from noon import Circle, Color, Scene, linear


class OrdinaryStylePlay(Scene):
    def construct(self):
        blue = Color(0.0, 0.4, 1.0)
        red = Color(1.0, 0.0, 0.0)
        green = Color(0.0, 1.0, 0.0)
        circle = Circle(radius=0.4).set_fill(blue, opacity=1.0)
        self.add(circle)

        self.play(
            circle.animate.set_fill(red, opacity=0.4).set_object_opacity(0.5),
            run_time=2.0,
            rate_func=linear,
        )
        assert self.time == 2.0

        circle.set_fill(green, opacity=1.0)
        circle.set_object_opacity(1.0)
        assert self.time == 2.0
