from noon import *


class ReactiveRelationships(Scene):
    def construct(self):
        horizontal = ValueTracker(-2.0)
        vertical = ValueTracker(-0.8)
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

        def follow_horizontal(dot):
            dot.set_x(horizontal.get_value())

        def follow_vertical(dot):
            dot.set_y(vertical.get_value())

        def follow_endpoints(line):
            line.match_points(Line(left.get_center(), right.get_center()))

        # Enroll the callback targets before the first play starts execution.
        left.add_updater(follow_horizontal)
        right.add_updater(follow_vertical)
        connector.add_updater(follow_endpoints)
        self.add(left, right, connector)
        self.play(
            FadeIn(title), FadeIn(caption), *[FadeIn(label) for label in labels],
            FadeIn(left), FadeIn(right), FadeIn(connector), run_time=0.6,
        )
        self.play(*[Create(guide) for guide in guides], run_time=0.9)
        self.wait(0.5)
        self.play(horizontal.animate.set_value(-0.5), vertical.animate.set_value(1.4), run_time=2.4, rate_func=smooth)
        self.wait(0.6)
        self.play(horizontal.animate.set_value(-3.0), vertical.animate.set_value(-1.5), run_time=2.4, rate_func=smooth)
        left.remove_updater(follow_horizontal)
        right.remove_updater(follow_vertical)
        connector.remove_updater(follow_endpoints)
        self.play(*[FadeOut(guide) for guide in guides], FadeOut(caption), FadeIn(resolved), run_time=0.6)
        self.wait(1.2)
