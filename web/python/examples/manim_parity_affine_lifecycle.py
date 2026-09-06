from noon import *


class AffineLifecycleExample(Scene):
    """Pair for Rust's ordinary_affine_lifecycle_program."""

    def construct(self):
        square = Square(1.0, fill_color=BLUE, fill_opacity=0.7)
        self.play(
            SpinInFromNothing(
                square,
                angle=PI / 2,
                point_color=RED,
                run_time=1.0,
                rate_func=smooth,
            )
        )
        self.play(ShrinkToCenter(square, run_time=1.0, rate_func=smooth))
