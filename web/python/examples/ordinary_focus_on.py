"""Fixed-point spotlight through shared Rust staging, with a later continuation."""
from noon import Color, FocusOn, Scene, Square, linear

class OrdinaryFocusOn(Scene):
    def construct(self):
        square = Square(side_length=1.0, fill_color=Color(0.0, 0.0, 1.0), fill_opacity=1.0, stroke_width=0.0)
        square.shift((-3.0, -2.0))
        self.add(square)
        self.wait(0.25)
        self.play(FocusOn((2.0, 1.0), opacity=0.8, color=Color(0.0, 1.0, 1.0)),
                  run_time=2.0, rate_func=linear)
        assert self.mobjects == [square]
        self.wait(0.25)
