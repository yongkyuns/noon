from noon import *


class OrdinaryRotating(Scene):
    def construct(self):
        rectangle = Rectangle(width=3.0, height=0.6, color=Color(0.0, 1.0, 1.0), fill_opacity=1.0, stroke_width=0)
        # Rotating defers its center; moving after construction remains valid.
        turn = Rotating(rectangle, run_time=2.0)
        rectangle.shift(2 * RIGHT + UP)
        self.add(rectangle)
        self.wait(0.25)
        self.play(turn)
        self.play(Rotate(rectangle, angle=PI / 2, axis=IN, rate_func=linear))
        self.wait(0.25)
