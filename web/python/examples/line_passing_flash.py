"""Exact-Line PassingFlash through the shared semantic composition path."""

from noon import CYAN, PI, Line, Scene, ShowPassingFlash, linear


class LinePassingFlash(Scene):
    def construct(self):
        line = (
            Line((-2.0, 0.0), (2.0, 0.0), color=CYAN, stroke_width=8.0)
            .scale((1.25, 0.75))
            .rotate(PI / 6.0)
            .shift((0.5, -0.5))
        )

        self.wait(0.25)
        self.play(
            ShowPassingFlash(line, time_width=0.25),
            run_time=2.0,
            rate_func=linear,
        )
        assert line not in self.mobjects

        self.add(line)
        assert line in self.mobjects
        self.wait(0.25)
