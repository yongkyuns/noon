from manim import *


class OrdinaryPathArrangement(Scene):
    def construct(self):
        a = Arc(radius=1, start_angle=-0.3, angle=1.8, num_components=3, color=WHITE)
        b = a.copy().set_color("#58c4dd")
        c, d = a.copy(), b.copy()
        row = VGroup(a, b).arrange(RIGHT, buff=0.4).move_to(LEFT * 2)
        grid = VGroup(c, d).arrange_in_grid(rows=2, cols=1, buff=(0.4, 0.4)).move_to(RIGHT * 2)
        self.add(row, grid)
        self.wait(0.2)
