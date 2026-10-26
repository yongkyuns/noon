"""Create two objects, arrange them, and animate a shared composition."""
from noon import *


class FirstScene(Scene):
    def construct(self):
        theme = Theme.dark()
        title = theme.text("Your first scene", role="title").to_edge(UP, buff=theme.spacing("page"))
        caption = theme.text("Create objects, then give them a little motion", role="caption").to_edge(DOWN, buff=theme.spacing("page"))
        circle = Circle(radius=0.75, color=theme.categorical(0), fill_opacity=0.8)
        square = Square(side_length=1.5, color=theme.categorical(1), fill_opacity=0.8)
        square.next_to(circle, RIGHT, buff=theme.spacing("large"))
        VGroup(circle, square).center()

        self.play(FadeIn(title), FadeIn(caption), run_time=0.7, rate_func=smooth)
        self.play(Create(circle), Create(square), run_time=1.4, rate_func=smooth)
        self.wait(0.5)
        self.play(
            circle.animate.shift(0.7 * UP),
            square.animate.rotate(PI / 4).shift(0.7 * DOWN),
            run_time=1.6, rate_func=smooth,
        )
        self.play(
            circle.animate.shift(0.7 * DOWN),
            square.animate.rotate(-PI / 4).shift(0.7 * UP),
            run_time=1.6, rate_func=smooth,
        )
        self.play(Indicate(circle), Indicate(square), run_time=1.0)
        self.wait(1.2)
