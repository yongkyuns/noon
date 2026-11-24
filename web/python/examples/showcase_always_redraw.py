"""Two shapes driven by authored values on the retained timeline."""
from noon import *


class AlwaysRedrawShowcase(Scene):
    def construct(self):
        clock = self.value_tracker(0)
        visibility = self.value_tracker(0)
        circle = Circle(radius=0.48, color=BLUE, fill=BLUE, fill_opacity=0.84)
        rectangle = Rectangle(width=1.15, height=0.78, color=TEAL, fill=TEAL, fill_opacity=0.84)
        self.add(circle, rectangle)
        self.bind_opacity(circle, visibility)
        self.bind_opacity(rectangle, visibility)
        self.bind_sampled(circle, clock, "position", [(0, -2.2, 0.15), (1, -1.15, 0.15), (2, -2.2, 0.15)])
        self.bind_sampled(circle, clock, "scale", [(0, 1, 1), (1, 0.82 / 0.48, 0.82 / 0.48), (2, 1, 1)])
        self.bind_sampled(rectangle, clock, "position", [(0, 2.2, 0.15), (1, 1.15, 0.15), (2, 2.2, 0.15)])
        self.bind_sampled(rectangle, clock, "rotation", [(0, 0), (1, 0.65), (2, -0.45)])

        title = Text("Redraw from animated values", font_size=32).shift(3.25 * UP)
        subtitle = Text("Animated values drive each shape.", font_size=20, color=GRAY).shift(2.65 * UP)
        labels = [
            Text("Circle · position + radius", font_size=18, color=BLUE).move_to((-2.2, 1.65, 0)),
            Text("Rectangle · position + angle", font_size=18, color=TEAL).move_to((2.2, 1.65, 0)),
        ]
        resolved = Text("Both shapes redraw as their values change.", font_size=21).shift(2.65 * UP)

        self.play(
            FadeIn(title), FadeIn(subtitle), *[FadeIn(label) for label in labels],
            visibility.animate.set_value(1), run_time=0.8, rate_func=smooth,
        )
        self.wait(0.4)
        self.play(clock.animate.set_value(1), run_time=2.4, rate_func=smooth)
        self.wait(0.5)
        self.play(clock.animate.set_value(2), run_time=2.4, rate_func=smooth)
        self.play(FadeOut(subtitle), FadeIn(resolved), run_time=0.5)
        self.wait(1.2)
