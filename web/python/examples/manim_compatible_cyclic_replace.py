# Source-compatible CyclicReplace example backed by shared Rust family Transform arcs.
from noon import BLUE, PINK, YELLOW, Circle, CyclicReplace, Group, LEFT, RIGHT, Scene, Square, UP


class Main(Scene):
    def construct(self):
        first = Square(0.7).set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(2 * LEFT)
        second = Circle(0.4).set_fill(PINK, opacity=0.9).set_stroke(opacity=0)
        third = Square(0.5).set_fill(YELLOW, opacity=0.9).set_stroke(opacity=0).shift(2 * RIGHT)
        self.add(first, second, third)
        family = Group(first, second, third)
        self.play(family.animate.shift(0.5 * RIGHT + 0.75 * UP), run_time=0.5)
        self.play(CyclicReplace(first, second, third), run_time=2.0)
