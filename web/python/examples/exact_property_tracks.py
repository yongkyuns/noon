"""Pair of the native Rust exact_property_tracks example, using ordinary builders."""

from noon import Circle, Color, PI, RIGHT, Rotate, Scene, Square, linear


class ExactPropertyTracks(Scene):
    def construct(self):
        circle = Circle(0.75, color=Color(1, 0, 0), fill_opacity=1).move_to([-2, 1, 0])
        square = Square(1.5, color=Color(0, 0, 1), fill_opacity=1).move_to([0, -1, 0])
        self.add(circle, square)
        self.play(
            circle.animate.shift(4 * RIGHT).set_object_opacity(0.25),
            Rotate(square, PI),
            run_time=2,
            rate_func=linear,
        )
