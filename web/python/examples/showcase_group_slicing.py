"""A slice selects existing members; unselected objects remain in place."""
from noon import *


class SelectingGroupMembers(Scene):
    def construct(self):
        theme = Theme.dark()
        title = theme.text("Animate part of a group", role="title").to_edge(UP, buff=theme.spacing("page"))
        items = VGroup(*[
            Square(side_length=0.8, color=theme.categorical(index), fill_opacity=0.8)
            for index in range(5)
        ]).arrange(RIGHT, buff=theme.spacing("large"))
        indices = [
            theme.text(str(index), role="label").next_to(item, DOWN, buff=theme.spacing("normal"))
            for index, item in enumerate(items)
        ]
        caption = theme.text("items[1:4] selects indices 1, 2, and 3", role="caption").to_edge(DOWN, buff=theme.spacing("page"))
        self.play(FadeIn(title), FadeIn(caption), run_time=0.7)
        self.play(Create(items), *[FadeIn(index) for index in indices], run_time=1.2)
        self.wait(0.5)
        selected = items[1:4]
        self.play(Indicate(selected), run_time=1.0)
        self.play(selected.animate.shift(0.9 * UP), run_time=1.5, rate_func=smooth)
        self.wait(1.0)
        self.play(selected.animate.shift(0.9 * DOWN), run_time=1.5, rate_func=smooth)
        self.wait(1.0)
