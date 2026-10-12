"""Async Python-worker counterpart of the ordinary LTS matrix fixture."""

from noon import *


class VectorSpaceLTS(LinearTransformationScene):
    async def construct(self):
        self.test_vector = Vector((2.0, 1.0), color=YELLOW)
        await self.add_vector(self.test_vector, animate=True)
        await self.apply_matrix([[0.0, 1.0], [1.0, 0.0]])
