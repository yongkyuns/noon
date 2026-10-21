"""Own root points and constructor intent across ordinary continuations."""
from noon import *


class FamilyRootPoints(Scene):
    async def construct(self):
        first, second = Circle(0.2).shift(LEFT * 2), Square(0.3).shift(LEFT * 3)
        points = [[0., 0., 0.], [0.1, 0., 0.], [0.2, 0.3, 0.],
                  [0.3, 0.4, 0.], [0.5, 0.6, 0.]]

        def exercise(group, vector):
            for count in (0, 1, 4, 5):
                assert group.set_points(points[:count]) is group
                assert vector.set_points(points[:count]) is vector
                assert group.get_num_points() == vector.get_num_points() == count
                assert group.has_points() is vector.has_points() is (count != 0)
                assert group.submobjects == vector.submobjects == [first, second]
            assert group.reset_points() is group
            assert vector.clear_points() is vector
            assert group.get_num_points() == vector.get_num_points() == 0

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
