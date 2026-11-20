from noon import *


class ReactiveRelationships(Scene):
    def construct(self):
        left = Dot(2 * LEFT + 0.8 * DOWN, radius=0.13, color=BLUE)
        right = Dot(2 * RIGHT + 0.8 * DOWN, radius=0.13, color=TEAL)
        connector = Line(left.get_center(), right.get_center(), color=YELLOW, stroke_width=4)
        guides = [
            Line(4 * LEFT + 0.8 * DOWN, 0.8 * DOWN, color=GRAY, stroke_width=1),
            Line(2 * RIGHT + 2 * DOWN, 2 * RIGHT + 2 * UP, color=GRAY, stroke_width=1),
        ]
        title = Text("Relationships that follow motion", font_size=29).shift(3.2 * UP)
        caption = Text("Animate two values. Update the dots and their connector.", font_size=20).shift(2.5 * UP)
        labels = [
            Text("horizontal driver", font_size=19, color=BLUE).move_to(2 * LEFT + 2.7 * DOWN),
            Text("vertical driver", font_size=19, color=TEAL).move_to(2 * RIGHT + 2.7 * DOWN),
        ]
        resolved = Text("The connector follows both endpoints.", font_size=22).shift(2.5 * UP)

        # The endpoint trajectories and the line morph share the same easing.
        def move_readings(left, right, connector, x, y):
            left_end, right_end = (x, -0.8, 0), (2, y, 0)
            return (
                left.animate.move_to(left_end),
                right.animate.move_to(right_end),
                Transform(connector, Line(left_end, right_end, color=YELLOW, stroke_width=4)),
            )

        for mobject in (left, right, connector):
            mobject.set_opacity(0)
        self.add(left, right, connector)
        self.play(
            FadeIn(title), FadeIn(caption), *[FadeIn(label) for label in labels],
            left.animate.set_opacity(1), right.animate.set_opacity(1),
            connector.animate.set_opacity(1), run_time=0.6, rate_func=smooth,
        )
        self.play(*[Create(guide) for guide in guides], run_time=0.9)
        self.wait(0.5)
        self.play(*move_readings(left, right, connector, -0.5, 1.4), run_time=2.4, rate_func=smooth)
        self.wait(0.6)
        self.play(*move_readings(left, right, connector, -3.0, -1.5), run_time=2.4, rate_func=smooth)
        self.play(*[FadeOut(guide) for guide in guides], FadeOut(caption), FadeIn(resolved), run_time=0.6)
        self.wait(1.2)
