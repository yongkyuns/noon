"""A slice selects existing members; unselected objects remain in place."""
from noon import *


class SelectingGroupMembers(Scene):
    def construct(self):
        title = Text("Animate part of a group", font_size=34).shift(2.9 * UP)
        colors = [BLUE, TEAL, GREEN, YELLOW, PINK]
        items = VGroup(*[
            Square(side_length=0.8, color=color).set_fill(color, opacity=0.8).move_to(((index - 2) * 1.8, 0, 0))
            for index, color in enumerate(colors)
        ])
        indices = [Text(str(index), font_size=24).move_to(((index - 2) * 1.8, -1, 0)) for index in range(len(items))]
        caption = Text("items[1:4] selects indices 1, 2, and 3", font_size=24).shift(2.6 * DOWN)
        self.play(FadeIn(title), FadeIn(caption), run_time=0.7)
        self.play(Create(items), *[FadeIn(index) for index in indices], run_time=1.2)
        self.wait(0.5)
        selected = items[1:4]
        self.play(Indicate(selected), run_time=1.0)
        self.play(selected.animate.shift(0.9 * UP), run_time=1.5, rate_func=smooth)
        self.wait(1.0)
        self.play(selected.animate.shift(0.9 * DOWN), run_time=1.5, rate_func=smooth)
        self.wait(1.0)
