"""Swap the axes twice and watch a tracked vector return to its start."""
from noon import *


class LinearAlgebra(LinearTransformationScene):
    async def construct(self):
        caption = Typst("Swap the coordinates")
        caption.move_to((0, 3.45, 0))
        self.add(caption)
        self.add_vector((2, 1), color=YELLOW)

        await self.wait(0.5)
        # The symmetric axis-swap matrix has the default zero path arc.
        await self.apply_matrix([[0, 1], [1, 0]])
        await self.wait(0.5)
        await self.apply_matrix([[0, 1], [1, 0]])
        await self.wait(0.5)
