from noon import *


def triangle():
    return VMobject().set_points_as_corners(
        [(-1, -1, 0), (1, -0.5, 0), (-0.25, 1, 0), (-1, -1, 0)]
    )


def kite():
    return VMobject().set_points_as_corners(
        [(0, -1, 0), (1.5, 0, 0), (0, 1, 0), (-0.5, 0, 0), (0, -1, 0)]
    )


class OrdinaryTransformMatchingShapes(Scene):
    def construct(self):
        source_triangle = (
            triangle().set_fill(PINK, opacity=0.9).set_stroke(opacity=0).shift(LEFT * 2)
        )
        source_kite = (
            kite().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(RIGHT * 2)
        )
        source = VGroup(source_triangle, source_kite)
        self.add(source)

        target_kite = (
            kite().set_fill(BLUE, opacity=0.9).set_stroke(opacity=0).shift(LEFT * 4)
        )
        target_triangle = (
            triangle().set_fill(PINK, opacity=0.9).set_stroke(opacity=0).shift(RIGHT * 4)
        )
        target = VGroup(target_kite, target_triangle)

        def assert_state(readded=False):
            expected_roots = (target, source) if readded else (target,)
            roots = self.mobjects
            assert len(roots) == len(expected_roots)
            assert all(actual is expected for actual, expected in zip(roots, expected_roots))
            for family, members in (
                (source, (source_triangle, source_kite)),
                (target, (target_kite, target_triangle)),
            ):
                actual = family.submobjects
                assert len(actual) == len(members)
                assert all(member is expected for member, expected in zip(actual, members))
            assert abs(target_kite.get_center().x + 3.5) < 1e-6
            assert abs(target_triangle.get_center().x - 4) < 1e-6
            assert abs(source_triangle.get_center().x + 2) < 1e-6
            assert abs(source_kite.get_center().x - 2.5) < 1e-6

        self.play(
            TransformMatchingShapes(source, target, run_time=1.0, rate_func=linear)
        )
        assert_state()
        self.play(Indicate(target, run_time=1.0))
        assert_state()
        self.add(source)
        assert_state(readded=True)
        self.wait(0.1)
        assert_state(readded=True)
