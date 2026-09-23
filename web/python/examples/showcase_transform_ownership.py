from noon import *


class TransformOwnership(Scene):
    def construct(self):
        rows = [1.45, 0.0, -1.45]
        names = ["Transform", "ReplacementTransform", "TransformFromCopy"]
        title = Text("After the morph, which object moves?", font_size=28).shift(3.2 * UP)
        labels = [
            Text(name, font_size=17).move_to(4.5 * LEFT + y * UP)
            for name, y in zip(names, rows)
        ]
        sources = [
            Square(side_length=0.9, color=BLUE, fill_opacity=0.7)
            .move_to(0.75 * LEFT + y * UP)
            for y in rows
        ]
        targets = [
            Circle(radius=0.48, color=TEAL, fill_opacity=0.7)
            .move_to(2.5 * RIGHT + y * UP)
            for y in rows
        ]
        captions = [
            Text(text, font_size=17, color=TEAL).move_to(4.8 * RIGHT + y * UP)
            for text, y in zip(["source moves", "target moves", "both remain"], rows)
        ]
        note = Text("The copy keeps its original square.", font_size=22).shift(2.9 * DOWN)

        self.play(FadeIn(title), *[FadeIn(label) for label in labels], run_time=0.6)
        self.play(*[Create(source) for source in sources], run_time=1.0)
        self.wait(0.6)
        self.play(
            Transform(sources[0], targets[0]),
            ReplacementTransform(sources[1], targets[1]),
            TransformFromCopy(sources[2], targets[2]),
            run_time=2.2, rate_func=smooth,
        )
        self.play(FadeIn(note), *[FadeIn(caption) for caption in captions], run_time=0.6)
        self.wait(0.6)
        # These are the public references to animate after each operation.
        self.play(
            sources[0].animate.shift(0.6 * UP),
            targets[1].animate.shift(0.6 * UP),
            targets[2].animate.shift(0.6 * UP),
            sources[2].animate.shift(0.6 * UP),
            run_time=1.4, rate_func=smooth,
        )
        self.wait(1.2)
