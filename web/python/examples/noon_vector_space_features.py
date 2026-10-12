"""Async Python-worker counterpart for LTS coordinate and ghost features."""

from noon import *


class VectorSpaceLTSFeatures(LinearTransformationScene):
    def __init__(self):
        super().__init__(
            include_foreground_plane=False,
            show_coordinates=True,
            show_basis_vectors=False,
            leave_ghost_vectors=True,
            background_plane_kwargs={
                "x_range": (-1.0, 1.0, 1.0),
                "y_range": (-1.0, 1.0, 1.0),
                "x_length": 4.0,
                "y_length": 4.0,
            },
        )

    async def construct(self):
        square = Square(side_length=0.5)
        square.move_to([0.5, 0.5, 0.0])
        self.add_transformable_mobject(square)
        vector = Vector((0.5, 0.25), color=YELLOW)
        self.add_vector(vector, animate=False)
        await self.wait(0.25)
        await self.apply_matrix([[0.0, 1.0], [1.0, 0.0]], run_time=0.5)
        await self.wait(0.25)
        await self.apply_matrix([[2.0, 0.0], [0.0, 1.0]], run_time=0.5)
