"""Captured list queries and current own-root ranks drive the same visible family."""
from noon import *


class FamilyMemberQueries(Scene):
    async def construct(self):
        red = Circle(.3, fill_color="#ff0000", fill_opacity=1, stroke_width=0).shift(LEFT * 3)
        blue = Square(.6, fill_color="#0000ff", fill_opacity=1, stroke_width=0).shift(LEFT)
        yellow = Circle(.3, fill_color="#ffff00", fill_opacity=1, stroke_width=0).shift(RIGHT)
        extra = Circle(.3, fill_color="#00ff00", fill_opacity=1, stroke_width=0).shift(RIGHT * 3)
        root = Group(red, blue, yellow)
        captured = iter(root)

        class Start:
            def __index__(self):
                root.add(extra)
                root.remove(red)
                return 0

        visible = root[slice(Start(), None, 2)]
        assert visible.submobjects == [red, yellow]
        assert list(captured) == [red, blue, yellow]
        assert root.split() == [blue, yellow, extra]
        points = Square(.5).shift(DOWN * 2).get_all_points().copy()
        self.add(visible)
        await self.wait(.25)
        selected = [root[0], root[1]]
        assert selected == [blue, yellow]
        visible.remove(*visible.submobjects)
        visible.add(*selected)
        await self.wait(.25)
        root.set_points(points)
        assert root.get_num_points() == 16 and len(root) == 4
        assert root.split() == [root, blue, yellow, extra]
        assert root[0] is root
        visible.remove(*visible.submobjects)
        visible.add(root[0])
        await self.wait(.25)
