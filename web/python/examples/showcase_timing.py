"""Compare simultaneous, sequential, and staggered entrances using identical rows."""
from noon import *


class AnimationTiming(Scene):
    def construct(self):
        theme = Theme.dark()
        title = theme.text("Together, in sequence, or staggered?", role="title").to_edge(UP, buff=theme.spacing("page"))
        labels = []
        rows = []
        for index, name in enumerate(("Together", "Sequence", "Staggered")):
            color = theme.categorical(index)
            labels.append(theme.text(name, role="label", color=color))
            rows.append(VGroup(*[
                Square(side_length=0.7, color=color, fill_opacity=0.8)
                for _ in range(4)
            ]).arrange(RIGHT, buff=theme.spacing("large")))
        VGroup(*rows).arrange(DOWN, buff=theme.spacing("large"))
        for label, row in zip(labels, rows):
            label.next_to(row, LEFT, buff=theme.spacing("large"))
        VGroup(*labels, *rows).center()
        caption = theme.text("Same four objects. Only their timing changes.", role="caption").to_edge(DOWN, buff=theme.spacing("page"))
        self.play(FadeIn(title), FadeIn(caption), *[FadeIn(label) for label in labels], run_time=0.8)
        self.play(*[FadeIn(item, shift=0.3 * UP) for item in rows[0]], run_time=1.6, rate_func=smooth)
        self.wait(0.4)
        self.play(Succession(*[FadeIn(item, shift=0.3 * UP, run_time=0.6) for item in rows[1]], rate_func=linear))
        self.wait(0.4)
        self.play(LaggedStartMap(FadeIn, rows[2], lag_ratio=0.4, run_time=2.4))
        self.wait(1.2)
