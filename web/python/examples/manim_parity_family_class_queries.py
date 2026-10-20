"""Group/VGroup public bases and root queries across a live continuation."""
from noon import *


class FamilyClassQueries(Scene):
    async def construct(self):
        assert Group.__bases__ == (Mobject,)
        assert VGroup.__bases__ == (VMobject,)
        assert VGroup.__mro__ == (VGroup, VMobject, Mobject, object)
        first, second = Circle(0.2), Square(0.3)
        group, vector = Group(first, second), VGroup(first, second)

        def observe():
            assert isinstance(group, Mobject) and not isinstance(group, VMobject)
            assert isinstance(vector, VMobject) and not isinstance(vector, Group)
            assert group.get_group_class() is Group
            assert vector.get_group_class() is VGroup
            assert group.get_num_points() == vector.get_num_points() == 0
            assert len(group) == len(vector) == 2
            assert group.split() == vector.split() == [first, second]
            assert list(vector.get_anchors()) == []

        observe()
        self.add(group, vector)
        await self.wait(0.2)
        observe()
        await self.wait(0.2)
