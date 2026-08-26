from noon import *


class ShowDrawBorderThenFill(Scene):
    def construct(self):
        self.play(DrawBorderThenFill(Square(fill_opacity=1, fill_color=ORANGE)))
