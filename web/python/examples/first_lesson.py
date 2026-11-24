"""Paired with noon::example_scenes::first_lesson; no external assets required."""

from noon import *


def step(scene, shapes):
    scene.play(shapes.animate.shift(0.75 * RIGHT), run_time=0.5, rate_func=linear)


class FirstLesson(Scene):
    def construct(self):
        title = Text("One scene, ordinary functions", font_size=36).shift(2.7 * UP)
        equation = MathTypst("x(t) = x_0 + v t", font_size=48).shift(1.6 * UP)
        self.add(title, equation)

        square = Square(side_length=0.8).set_fill(BLUE, opacity=1)
        circle = Circle(radius=0.4).set_fill(ORANGE, opacity=1)
        square.shift(0.8 * LEFT + 0.4 * DOWN)
        circle.shift(0.8 * RIGHT + 0.4 * DOWN)
        shapes = VGroup(square, circle)
        self.add(shapes)
        self.wait(0.5)
        for _ in range(2):
            step(self, shapes)

        caption = Text("Two steps, one shared scene", font_size=24).shift(2 * DOWN)
        self.add(caption)
        self.wait(0.5)
