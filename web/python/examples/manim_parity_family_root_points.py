"""Own root points and constructor intent across ordinary continuations."""
from noon import *
import numpy as np


class FamilyRootPoints(Scene):
    async def construct(self):
        first, second = Circle(0.2).shift(LEFT * 2), Square(0.3).shift(LEFT * 3)
        points = [[0., 0., 0.], [0.1, 0., 0.], [0.2, 0.3, 0.],
                  [0.3, 0.4, 0.], [0.5, 0.6, 0.]]

        def exercise(group, vector):
            for count in (0, 1, 4, 5):
                assert group.set_points(points[:count]) is group
                assert vector.clear_points() is vector
                assert vector.append_points(np.array(points[:count]).reshape((-1, 3))) is vector
                assert group.get_num_points() == vector.get_num_points() == count
                assert group.has_points() is vector.has_points() is (count != 0)
                assert group.submobjects == vector.submobjects == [first, second]
                for root in (group, vector):
                    expected = [root, first, second] if count else [first, second]
                    assert len(root) == len(expected)
                    assert root.split() == list(root) == expected
                    assert root[0] is expected[0] and root[-1] is second
            assert group.reset_points() is group
            assert vector.clear_points() is vector
            assert group.get_num_points() == vector.get_num_points() == 0
            for root in (group, vector):
                captured = iter(root)
                root.add()  # CE replaces the list even for an empty add.
                root.remove(first)
                assert list(captured) == [first, second]
                root.add_to_back(first)
                root.set_points(points[:4])
                captured = iter(root)
                root.reset_points()
                assert list(captured) == [root, first, second]
                assert root.submobjects == [first, second]
                captured = iter(root)
                clone = root.copy()
                assert list(captured) == [first, second]
                copied = list(clone)
                assert len(copied) == 2
                assert all(value is not original for value, original in zip(copied, (first, second)))
                clone.remove(*copied)
                assert list(root) == [first, second] and len(clone) == 0

        group, vector = Group(first, second), VGroup(first, second)
        exercise(group, vector)
        self.add(group, vector)
        assert self.mobjects == [group, vector]
        await self.wait(0.25)
        exercise(group, vector)
        # These roots are constructed after the shared execution is returned.
        later_group, later_vector = Group(first, second), VGroup(first, second)
        exercise(later_group, later_vector)
        self.add(later_group, later_vector)
        assert self.mobjects == [later_group, later_vector]
        await self.wait(0.25)
        exercise(later_group, later_vector)
        await self.wait(0.25)
