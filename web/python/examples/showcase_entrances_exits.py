from noon import *


class EntrancesAndExits(Scene):
    def construct(self):
        positions = [-4.5, -1.5, 1.5, 4.5]
        names = ["Fade", "Draw", "Grow", "Spin"]
        title = Text("How an object arrives", font_size=30).shift(3.1 * UP)
        labels = [
            Text(name, font_size=22).move_to(x * RIGHT + 1.5 * UP)
            for name, x in zip(names, positions)
        ]

        def squares():
            return [
                Square(side_length=1.35, color=BLUE, fill_opacity=0.65)
                .move_to(x * RIGHT)
                for x in positions
            ]

        shapes = squares()
        entrances = Text("Four entrances, one shared duration", font_size=22).shift(2.4 * DOWN)
        exits = Text("FadeOut / Uncreate / ShrinkToCenter / ShrinkToCenter", font_size=17).shift(2.4 * DOWN)
        resolved = Text("Same shape. Different ways to introduce it.", font_size=22).shift(2.4 * DOWN)

        self.play(FadeIn(title), FadeIn(entrances), *[FadeIn(label) for label in labels], run_time=0.7)
        self.play(
            FadeIn(shapes[0]), Create(shapes[1]),
            GrowFromCenter(shapes[2]), SpinInFromNothing(shapes[3]),
            run_time=2.2, rate_func=smooth,
        )
        self.wait(0.8)
        self.play(FadeOut(entrances), FadeIn(exits), run_time=0.5)
        self.play(
            FadeOut(shapes[0]), Uncreate(shapes[1]),
            ShrinkToCenter(shapes[2]), ShrinkToCenter(shapes[3]),
            run_time=2.2, rate_func=smooth,
        )
        self.wait(0.5)
        # Fresh objects make the closing composition independent of exit state.
        self.play(FadeOut(exits), FadeIn(resolved), *[FadeIn(shape) for shape in squares()], run_time=1.2)
        self.wait(1.4)
