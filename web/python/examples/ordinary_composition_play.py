"""Flat ordinary Parallel and Succession through one retained Rust session."""

from noon import Circle, Color, Scene, Succession, linear


class OrdinaryCompositionPlay(Scene):
    def construct(self):
        white = Color(1.0, 1.0, 1.0)
        red = Color(1.0, 0.0, 0.0)
        blue = Color(0.0, 0.0, 1.0)
        green = Color(0.0, 1.0, 0.0)
        left = Circle(radius=0.4).set_fill(white, opacity=1.0).shift((-2.0, 0.0, 0.0))
        right = Circle(radius=0.4).set_fill(white, opacity=1.0).shift((2.0, 0.0, 0.0))
        self.add(left, right)

        self.play(
            left.animate(run_time=2.0, rate_func=linear).shift((0.0, 1.0, 0.0)),
            right.animate(run_time=2.0, rate_func=linear).shift((0.0, -1.0, 0.0)),
            rate_func=linear,
        )
        assert self.time == 2.0
        assert tuple(left.get_center()) == (-2.0, 1.0)
        assert tuple(right.get_center()) == (2.0, -1.0)

        self.play(
            Succession(
                left.animate(run_time=1.0, rate_func=linear).set_fill(red, opacity=1.0),
                right.animate(run_time=1.0, rate_func=linear).set_fill(blue, opacity=1.0),
                rate_func=linear,
            ),
            rate_func=linear,
        )
        assert self.time == 4.0
        assert tuple(left.get_center()) == (-2.0, 1.0)
        assert tuple(right.get_center()) == (2.0, -1.0)

        # Root completion releases both mapped drivers before this normal edit.
        left.set_fill(green, opacity=1.0)
