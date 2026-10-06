"""Grow a vector, then swap its coordinates while faded copies retain each step."""
from noon import *

class LinearAlgebra(LinearTransformationScene):
    def __init__(self):
        super().__init__(leave_ghost_vectors=True)

    async def construct(self):
        await prepare_latex()
        self.background_plane.add_coordinates()
        caption = Typst("Swap the coordinates")
        caption.move_to((0, 3.45, 0))
        self.add(caption)
        await self.add_vector((2, 1), color=YELLOW, animate=True)

        await self.wait(0.5)
        # The symmetric axis-swap matrix has the default zero path arc.
        await self.apply_matrix([[0, 1], [1, 0]])
        await self.wait(0.5)
        await self.apply_matrix([[0, 1], [1, 0]])
        await self.wait(0.5)
