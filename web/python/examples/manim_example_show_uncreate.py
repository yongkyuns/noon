from noon import *


class ShowUncreate(Scene):
    def construct(self):
        self.play(Uncreate(Square()))
