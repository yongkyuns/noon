"""Ordinary object children and copies share native identity and live publication."""
from manim import *


def assert_depth(root, own, minimum, maximum):
    anchors = root.get_anchors()
    assert len(anchors) > 0
    assert all(float(point[2]) == own for point in anchors)
    assert float(root.get_z(IN)) == minimum
    assert float(root.get_z(OUT)) == maximum
    assert float(root.get_center()[2]) == (minimum + maximum) / 2


def assert_mixed_content_bounds():
    # These detached objects exercise public content-layout queries without
    # changing the visible membership/copy scene or its native pixel pair.
    text = Text("I").shift(3 * LEFT)
    circle = Circle(.2).shift(2 * RIGHT)
    text.shift((0., 0., 1.125))
    circle.shift((0., 0., -3.5))
    for method in (text.get_zenith, text.get_nadir):
        assert float(method()[2]) == 1.125
    expected_x = (float(text.get_left()[0]) + float(circle.get_right()[0])) / 2
    for constructor in (Group, VGroup):
        family = constructor(text, circle)
        center = family.get_center()
        assert abs(float(center[0]) - expected_x) < 1e-6
        assert abs(float(center[1])) < 1e-6
        assert float(family.get_zenith()[2]) == 1.125
        assert float(family.get_nadir()[2]) == -3.5


class ObjectMembers(Scene):
    def construct(self):
        root = Circle(.4)
        a, b, c = Square(.3).shift(LEFT), Circle(.3).shift(RIGHT), Square(.4).shift(UP)
        root.shift((0., 0., 1.125))
        a.shift((0., 0., 2.25))
        b.shift((0., 0., -3.5))
        c.shift((0., 0., 4.75))
        assert root.add(a, b, a) is root
        assert root.submobjects == [b, a]
        captured = iter(root)
        root.add(c)
        assert list(captured) == [root, b, a]
        root.remove(a)
        captured = iter(root)
        root.add_to_back(c, b, c)
        assert list(captured) == [root]
        assert root.submobjects == [c, b]
        root.metadata_alias = b
        assert_mixed_content_bounds()
        self.add(root)
        self.wait(.25)
        assert_depth(root, 1.125, -3.5, 4.75)
        root.submobjects = [a, b]
        root.shift((0., 0., .5))
        assert_depth(root, 1.625, -3., 2.75)
        assert_depth(c, 4.75, 4.75, 4.75)
        copied = root.copy()
        assert copied.metadata_alias is copied.submobjects[1]
        assert copied.submobjects[0] is not a
        copied.shift(DOWN)
        assert root.submobjects == [a, b]
        self.add(copied)
        self.wait(.25)
        assert_depth(copied, 1.625, -3., 2.75)
