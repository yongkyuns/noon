from noon import *


class OrdinaryTimedComposition(Scene):
    def construct(self):
        first, second, third = (
            Square(side_length=1).set_fill(BLUE, opacity=0.7).shift(RIGHT * x)
            for x in (-2, 0, 2)
        )
        self.play(Succession(
            Add(first, run_time=0.2),
            Succession(
                Wait(0.2),
                Add(second, run_time=0.2),
                Wait(0.2),
                rate_func=linear,
            ),
            Add(third, run_time=0.2),
            rate_func=smooth,
        ))
        self.wait(0.25)
