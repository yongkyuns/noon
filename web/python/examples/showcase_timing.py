"""Compare simultaneous, sequential, and staggered entrances using identical rows."""
from noon import *


class AnimationTiming(Scene):
    def construct(self):
        title = Text("Together, in sequence, or staggered?", font_size=32).shift(3 * UP)
        labels = []
        rows = []
        for name, y, color in [("Together", 1.5, BLUE), ("Sequence", 0, GREEN), ("Staggered", -1.5, PINK)]:
            labels.append(Text(name, font_size=24, color=color).move_to((-4.3, y, 0)))
            rows.append(VGroup(*[
                Square(side_length=0.7, color=color).set_fill(color, opacity=0.8).move_to((x, y, 0))
                for x in (-1.5, 0, 1.5, 3)
            ]))
        caption = Text("Same four objects. Only their timing changes.", font_size=22).shift(2.8 * DOWN)
        self.play(FadeIn(title), FadeIn(caption), *[FadeIn(label) for label in labels], run_time=0.8)
        self.play(*[FadeIn(item, shift=0.3 * UP) for item in rows[0]], run_time=1.6, rate_func=smooth)
        self.wait(0.4)
        self.play(Succession(*[FadeIn(item, shift=0.3 * UP, run_time=0.6) for item in rows[1]], rate_func=linear))
        self.wait(0.4)
        self.play(LaggedStartMap(FadeIn, rows[2], lag_ratio=0.4, run_time=2.4))
        self.wait(1.2)
