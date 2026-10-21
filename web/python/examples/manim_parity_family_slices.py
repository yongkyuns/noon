"""Native slice capture and Python protocol timing across continuations."""
from noon import *


class FamilySlices(Scene):
    async def construct(self):
        points = [[0., 0., 0.], [0.1, 0., 0.], [0.2, 0.3, 0.], [0.3, 0.4, 0.]]
        def exercise(root, a, b, extra):
            root.set_points(points)
            assert root[:].submobjects == [root, a, b]
            assert root[1:].submobjects == [a, b]
            assert root[::-1].submobjects == [b, a, root]
            class ClearPoints:
                def __index__(self):
                    root.set_points([])
                    return 0
            assert root[slice(ClearPoints(), 3)].submobjects == [root, a, b]
            class Append:
                def __index__(self):
                    root.add(extra)
                    return 0
            assert root[slice(Append(), 3)].submobjects == [a, b]
            assert root.submobjects == [a, b, extra]
            root.remove(extra)
            class Remove:
                def __index__(self):
                    root.remove(a)
                    return 0
            assert root[slice(Remove(), 3)].submobjects == [b]
            root.add_to_back(a)
            assert root[::10**1000].submobjects == [a]
            assert root[::-10**1000].submobjects == [b]
            events = []
            class Routed(Group):
                def get_group_class(self):
                    events.append("class")
                    return lambda *members: tuple(members)
            routed = Routed(a, b)
            assert routed[:1] == (a,)
            assert events == ["class"]
            assert routed[0] is a and events == ["class"]
            return root

        families = []
        for kind in (Group, VGroup):
            a, b, extra = Circle(.2), Square(.3), Circle(.4)
            root = kind(a, b)
            exercise(root, a, b, extra)
            families.append((root, a, b, extra))
            self.add(root)
        await self.wait(.25)
        for args in families:
            exercise(*args)
        await self.wait(.25)
