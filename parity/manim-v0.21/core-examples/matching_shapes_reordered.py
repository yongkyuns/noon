from manim import *


class MatchingShapesReordered(Scene):
    """Distinct shapes cross by shape key, then the original target is appended.

    The two-second linear animation puts every quarter on a 30 fps frame.
    The hold provides a real frame after cleanup, not an extrapolated endpoint.
    """

    def construct(self):
        def triangle(x, color):
            return Polygon(
                (-1, -1, 0), (1, -0.5, 0), (-0.25, 1, 0),
                color=color, fill_opacity=0.8, stroke_width=0,
            ).shift(x * RIGHT)

        def kite(x, color):
            return Polygon(
                (0, -1, 0), (1.5, 0, 0), (0, 1, 0), (-0.5, 0, 0),
                color=color, fill_opacity=0.8, stroke_width=0,
            ).shift(x * RIGHT)

        before = Circle(radius=1, color=WHITE, fill_opacity=0.6).shift(4 * LEFT)
        after = Circle(radius=1, color=WHITE, fill_opacity=0.6).shift(4 * RIGHT)
        source = VGroup(triangle(-2, BLUE), kite(2, RED))
        target = VGroup(kite(-4, YELLOW), triangle(4, GREEN))
        self.add(before, source, after)
        self.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
        self.wait(0.2)
