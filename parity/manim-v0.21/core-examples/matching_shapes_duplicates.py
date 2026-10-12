from manim import *


def triangle(x, y, color, scale=1.0):
    s = scale
    return VMobject(
        color=color, fill_opacity=0.8, stroke_width=0,
    ).set_points_as_corners([
        (-1 * s, -1 * s, 0),
        (1 * s, -0.5 * s, 0),
        (-0.25 * s, 1 * s, 0),
        (-1 * s, -1 * s, 0),
    ]).shift(x * RIGHT + y * UP)


def rotated_triangle(x, y, color):
    # Exact 90-degree rotation of triangle(). Normalized matching is translation/
    # scale invariant but intentionally rotation-sensitive, so this is a mismatch.
    return VMobject(
        color=color, fill_opacity=0.8, stroke_width=0,
    ).set_points_as_corners([
        (1, -1, 0),
        (0.5, 1, 0),
        (-1, -0.25, 0),
        (1, -1, 0),
    ]).shift(x * RIGHT + y * UP)


def kite(x, y, color):
    return VMobject(
        color=color, fill_opacity=0.8, stroke_width=0,
    ).set_points_as_corners([
        (0, -1, 0),
        (1.2, 0, 0),
        (0, 1, 0),
        (-0.6, 0, 0),
        (0, -1, 0),
    ]).shift(x * RIGHT + y * UP)


def assert_completed(scene, source, source_members, target, target_members, completed_waits=0):
    roots = scene.mobjects
    assert 1 <= len(roots) <= 1 + completed_waits
    assert roots[0] is target, "matching cleanup must admit the original target root"
    assert all(
        type(root) is Mobject and root.get_num_points() == 0 and not root.submobjects
        for root in roots[1:]
    ), "only inert Manim Wait placeholders may follow the target"
    assert all(
        actual is expected
        for actual, expected in zip(source.submobjects, source_members)
    ) and len(source.submobjects) == len(source_members)
    assert all(
        actual is expected
        for actual, expected in zip(target.submobjects, target_members)
    ) and len(target.submobjects) == len(target_members)


class MatchingShapesDuplicateGrowth(Scene):
    """Two equal keys grow to three; source-only and target-only keys fade."""

    def construct(self):
        source = VGroup(
            triangle(-4, 1.5, BLUE, 1.0),
            triangle(-1, 1.5, GREEN, 1.25),
            rotated_triangle(4, 1.5, RED),
        )
        target = VGroup(
            triangle(-4, -1.5, YELLOW, 0.75),
            triangle(-1, -1.5, PINK, 1.1),
            triangle(2, -1.5, BLUE, 1.35),
            kite(4, -1.5, WHITE),
        )
        source_members = tuple(source.submobjects)
        target_members = tuple(target.submobjects)
        self.add(source)
        self.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
        assert_completed(self, source, source_members, target, target_members)
        self.wait(0.1)
        assert_completed(self, source, source_members, target, target_members, 1)
        self.wait(0.1)
        assert_completed(self, source, source_members, target, target_members, 2)


class MatchingShapesDuplicateShrink(Scene):
    """Three equal keys shrink to two; both default mismatch fade directions occur."""

    def construct(self):
        source = VGroup(
            triangle(-4, 1.5, BLUE, 0.8),
            triangle(-1, 1.5, GREEN, 1.0),
            triangle(2, 1.5, PINK, 1.3),
            kite(4, 1.5, RED),
        )
        target = VGroup(
            triangle(-4, -1.5, YELLOW, 1.2),
            triangle(-1, -1.5, BLUE, 0.9),
            rotated_triangle(4, -1.5, WHITE),
        )
        source_members = tuple(source.submobjects)
        target_members = tuple(target.submobjects)
        self.add(source)
        self.play(TransformMatchingShapes(source, target), run_time=2, rate_func=linear)
        assert_completed(self, source, source_members, target, target_members)
        self.wait(0.1)
        assert_completed(self, source, source_members, target, target_members, 1)
        self.wait(0.1)
        assert_completed(self, source, source_members, target, target_members, 2)
