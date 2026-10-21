"""Ordinary object queries use the same native member engine as Group/VGroup."""
from noon import *
import numpy as np


class ObjectQueries(Scene):
    async def construct(self):
        points = [[0., 0., 0.], [.1, 0., 0.], [.2, .3, 0.], [.3, .4, 0.], [.5, .6, 0.]]
        def exercise(root):
            for count in (0, 1, 4, 5):
                root.apply_over_attr_arrays(lambda _: np.asarray(points[:count], dtype=float).reshape((-1, 3)))
                expected = [root] if count else []
                assert len(root) == bool(root) == bool(count)
                assert list(root) == root.split() == expected
                assert root[:].submobjects == expected
                assert root[::-1].submobjects == expected
                if count:
                    assert root[0] is root and root[-1] is root
                captured = iter(root)
                root.reset_points()
                assert list(captured) == expected
            root.apply_over_attr_arrays(lambda _: np.asarray(points[:4], dtype=float))
            captured = iter(root)
            copied = root.copy()
            assert list(captured) == [root]
            assert copied is not root and list(copied) == [copied]
        roots = [Mobject(), VMobject(), Circle(.2), Square(.3), VectorizedPoint()]
        for root in roots:
            exercise(root)
        self.add(*roots)
        await self.wait(.25)
        for root in roots:
            exercise(root)
        await self.wait(.25)
