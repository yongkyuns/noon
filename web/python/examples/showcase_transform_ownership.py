from noon import *


class TransformAndCopy(Scene):
    def construct(self):
        rows = [1.2, -1.2]
        title = Text("Transform an object or its copy", font_size=29).shift(3.2 * UP)
        labels = [
            Text(name, font_size=19).move_to(4.4 * LEFT + y * UP)
            for name, y in zip(["Transform", "copy() + Transform"], rows)
        ]
        source = Square(side_length=1.0, color=BLUE, fill_opacity=1.0).move_to(LEFT + rows[0] * UP)
        original = Square(side_length=1.0, color=BLUE, fill_opacity=1.0).move_to(LEFT + rows[1] * UP)
        copied = original.copy()
        targets = [
            Circle(radius=0.5, color=TEAL, fill_opacity=1.0).move_to(2.3 * RIGHT + y * UP)
            for y in rows
        ]
        captions = [
            Text(text, font_size=18).move_to(4.9 * RIGHT + y * UP)
            for text, y in zip(["same object", "separate copy"], rows)
        ]
        note = Text("The original stays blue; only the transformed objects change.", font_size=20).shift(2.8 * DOWN)

        self.play(FadeIn(title), *[FadeIn(label) for label in labels], run_time=0.6)
        self.play(Create(source), Create(original), run_time=1.0)
        self.wait(0.6)
        # Transform changes its input. A separate copy keeps the original intact.
        self.play(Transform(source, targets[0]), Transform(copied, targets[1]), run_time=2.2, rate_func=smooth)
        self.play(FadeIn(note), *[FadeIn(caption) for caption in captions], run_time=0.6)
        self.wait(0.6)
        self.play(
            source.animate.shift(0.6 * RIGHT).set_color(PINK),
            copied.animate.shift(0.6 * RIGHT).set_color(PINK),
            run_time=1.4, rate_func=smooth,
        )
        self.wait(1.2)
