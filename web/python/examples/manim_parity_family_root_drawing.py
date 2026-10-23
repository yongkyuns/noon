"""Held own content proves Group/VGroup drawing intent and child painter order."""
from noon import *
import numpy as np


def fit_depth(group, expected):
    extent = group.depth
    group.stretch_to_fit_depth(2., about_point=np.zeros(3))
    group.scale_to_fit_depth(4., about_point=np.array([-2., 0., 0.]))
    expected[:, 0] = (expected[:, 0] + 2.) * 2. - 2.
    expected[:, 1] *= 2.
    expected[:, 2] *= 4. / extent
    assert group.depth == 4.


class FamilyRootDrawing(Scene):
    async def construct(self):
        left = Circle(0.2, fill_color="#ffff00", fill_opacity=1, stroke_width=0).shift(LEFT * 2)
        right = Circle(0.2, fill_color="#ffff00", fill_opacity=1, stroke_width=0).shift(RIGHT * 2)
        left_seed, right_seed = left.copy(), right.copy()
        left_points = Square(1.2).shift(LEFT * 2).get_all_points().copy()
        right_points = Square(1.2).shift(RIGHT * 2).get_all_points().copy()
        left_points[:, 2] = np.arange(1, len(left_points) + 1) / 32.
        left_source_points = left_points.copy()
        group, vector = Group(left), VGroup(right)
        alignment_peer, alignment_leaf = VGroup(), Circle(0.7)
        vector.align_points(alignment_peer)
        group.set_points(left_points.copy())
        group.stretch(2., dim=2, about_point=np.zeros(3)).shift([0., 0., .25])
        left_points[:, 2] = left_points[:, 2] * 2. + .25
        fit_depth(group, left_points)
        vector.append_points(right_points[:8])
        vector.append_points(right_points[8:])
        vector.set_fill("#0000ff", opacity=1, family=False)
        vector.set_stroke(width=0, family=False)
        vector.align_points(alignment_leaf)
        vector.rotate(.37, about_point=[2., 0., 0.]).stretch(
            1.1234567890123, 0, about_point=[2., 0., 0.])
        vector.scale([.9, 1.1, 1.25], about_point=[2., 0., .5])
        self.add(group, vector)
        assert group.get_num_points() == 16 and vector.get_num_points() == 32
        np.testing.assert_array_equal(group.get_all_points()[:16], left_points)
        await self.wait(0.25)
        group.reset_points()
        vector.reset_points()
        vector.align_points(alignment_peer)
        assert group.get_num_points() == vector.get_num_points() == 0
        assert group.submobjects == [left] and vector.submobjects == [right]
        await self.wait(0.25)
        # Restore the original children and input before replaying family fits.
        left.become(left_seed)
        right.become(right_seed)
        left_points = left_source_points.copy()
        group.set_points(left_points.copy())
        group.stretch(2., dim=2, about_point=np.zeros(3)).shift([0., 0., .25])
        left_points[:, 2] = left_points[:, 2] * 2. + .25
        fit_depth(group, left_points)
        vector.append_points(right_points)
        vector.align_points(alignment_leaf)
        vector.rotate(.37, about_point=[2., 0., 0.]).stretch(
            1.1234567890123, 0, about_point=[2., 0., 0.])
        vector.scale([.9, 1.1, 1.25], about_point=[2., 0., .5])
        assert group.get_num_points() == 16 and vector.get_num_points() == 32
        np.testing.assert_array_equal(group.get_all_points()[:16], left_points)
        assert group.submobjects == [left] and vector.submobjects == [right]
        await self.wait(0.25)
