"""Held own content proves Group/VGroup drawing intent and child painter order."""
from noon import *


class FamilyRootDrawing(Scene):
    async def construct(self):
        left = Circle(0.2, fill_color="#ffff00", fill_opacity=1, stroke_width=0).shift(LEFT * 2)
        right = Circle(0.2, fill_color="#ffff00", fill_opacity=1, stroke_width=0).shift(RIGHT * 2)
        left_points = Square(1.2).shift(LEFT * 2).points.copy()
        right_points = Square(1.2).shift(RIGHT * 2).points.copy()
        group, vector = Group(left), VGroup(right)
        group.set_points(left_points)
        vector.set_points(right_points)
        vector.set_fill("#0000ff", opacity=1, family=False)
        vector.set_stroke(width=0, family=False)
        self.add(group, vector)
        assert group.get_num_points() == vector.get_num_points() == 16
        await self.wait(0.25)
        group.clear_points()
        vector.clear_points()
        assert group.get_num_points() == vector.get_num_points() == 0
        assert group.submobjects == [left] and vector.submobjects == [right]
        await self.wait(0.25)
        group.set_points(left_points)
        vector.set_points(right_points)
        assert group.get_num_points() == vector.get_num_points() == 16
        assert group.submobjects == [left] and vector.submobjects == [right]
        await self.wait(0.25)
