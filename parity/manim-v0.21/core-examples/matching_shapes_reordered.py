from manim import *


class MatchingShapesReordered(Scene):
    """Distinct shapes cross by shape key, then the original target is appended.

    The two-second play puts every quarter on a 30 fps frame.
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
        source_members = tuple(source.submobjects)
        target_members = tuple(target.submobjects)

        def assert_completed_state(completed_waits=0):
            # Observe the public API: rendering alone cannot prove that Python
            # wrappers follow the original target into the completed Rust scene.
            roots = self.mobjects
            assert 3 <= len(roots) <= 3 + completed_waits and all(
                actual is expected for actual, expected in zip(roots, (before, after, target))
            ), "matching cleanup must append the original target after surviving roots"
            # ManimCE 0.21.0 appends a plain empty Mobject for each Wait. These
            # are not authored roots; Noon need not reproduce internal dummies.
            # Do not filter real geometry, families, duplicates, or ordering errors.
            assert all(
                type(extra) is Mobject and not extra.submobjects
                and extra.width == 0 and extra.height == 0
                for extra in roots[3:]
            ), "only empty Wait placeholders may follow the original target"
            for family, members in ((source, source_members), (target, target_members)):
                actual = family.submobjects
                assert len(actual) == len(members) and all(
                    member is expected for member, expected in zip(actual, members)
                ), "matching cleanup must preserve source and target member identities"
            # get_center observes geometry bounds, not transform translation:
            # the asymmetric kite has a local x-center of 0.5.
            assert abs(target_members[0].get_center()[0] + 3.5) < 1e-6
            assert abs(target_members[1].get_center()[0] - 4.0) < 1e-6
            assert abs(source_members[0].get_center()[0] + 2.0) < 1e-6
            assert abs(source_members[1].get_center()[0] - 2.5) < 1e-6

        self.add(before, source, after)
        self.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
        assert_completed_state()
        # Cairo emits only one PNG per static wait, regardless of its duration.
        self.wait(0.1)
        assert_completed_state(completed_waits=1)
        self.wait(0.1)
        assert_completed_state(completed_waits=2)
