"""Async Python-worker counterpart of the ordinary LTS matrix fixture."""

from noon import *


class VectorSpaceLTS(LinearTransformationScene):
    async def construct(self):
        self.test_vector = self.add_vector((2.0, 1.0))
        await self.apply_matrix([[0.0, 1.0], [1.0, 0.0]])
