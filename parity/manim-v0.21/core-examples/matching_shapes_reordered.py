from manim import *


class MatchingShapesReordered(Scene):
    """Distinct shapes cross by shape key, then the original target is appended.

    The two-second linear animation puts every quarter on a 30 fps frame.
    Separate holds materialize both cleanup and post-cleanup reference frames.
    """

    def construct(self):
        # Use the common public path API, not the unexposed Polygon constructor.
        # Repeating the first vertex preserves the same closed contour in both engines.
        def triangle(x, color):
            return VMobject(
                color=color, fill_opacity=0.8, stroke_width=0,
            ).set_points_as_corners([
                (-1, -1, 0), (1, -0.5, 0), (-0.25, 1, 0), (-1, -1, 0),
            ]).shift(x * RIGHT)

        def kite(x, color):
            return VMobject(
                color=color, fill_opacity=0.8, stroke_width=0,
            ).set_points_as_corners([
                (0, -1, 0), (1.5, 0, 0), (0, 1, 0), (-0.5, 0, 0), (0, -1, 0),
            ]).shift(x * RIGHT)

        before = Circle(radius=1, color=WHITE, fill_opacity=0.6).shift(4 * LEFT)
        after = Circle(radius=1, color=WHITE, fill_opacity=0.6).shift(4 * RIGHT)
        source = VGroup(triangle(-2, BLUE), kite(2, RED))
        target = VGroup(kite(-4, YELLOW), triangle(4, GREEN))
        self.add(before, source, after)
        self.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
        # Cairo emits only one PNG per static wait, regardless of its duration.
        self.wait(0.1)
        self.wait(0.1)
