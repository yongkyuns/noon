from noon import *


class FocusOnPoint(Scene):
    def construct(self):
        self.play(FocusOn(2 * RIGHT))
