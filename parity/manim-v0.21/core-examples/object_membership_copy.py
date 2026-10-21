"""Ordinary object children and copies share native identity and live publication."""
from manim import *


class ObjectMembers(Scene):
    def construct(self):
        root = Circle(.4)
        a, b, c = Square(.3).shift(LEFT), Circle(.3).shift(RIGHT), Square(.4).shift(UP)
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
        self.add(root)
        self.wait(.25)
        root.submobjects = [a, b]
        copied = root.copy()
        assert copied.metadata_alias is copied.submobjects[1]
        assert copied.submobjects[0] is not a
        copied.shift(DOWN)
        assert root.submobjects == [a, b]
        self.add(copied)
        self.wait(.25)
