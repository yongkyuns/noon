from noon import *


class OrdinaryTextWrite(Scene):
    def construct(self):
        moving = Text("MOVE").shift(2 * LEFT + DOWN)
        writing = Text("WRITE").shift(LEFT + UP)
        self.play(
            moving.animate.shift(2 * RIGHT),
            Write(writing),
            run_time=2,
            rate_func=linear,
        )
        assert abs(moving.get_center()[0]) < 1e-6
        self.play(Unwrite(writing), run_time=1, rate_func=linear)
        self.wait(0.25)
