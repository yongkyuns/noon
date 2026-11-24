"""Grow a vector, then swap its coordinates while faded copies retain each step."""
from noon import *

class LinearAlgebra(LinearTransformationScene):
    def __init__(self):
        super().__init__(leave_ghost_vectors=True)

    def construct(self):
        self.background_plane.add_coordinates()
        caption = Typst("Swap the coordinates")
        caption.move_to((0, 3.45, 0))
        self.add(caption)
        self.add_vector((2, 1), color=YELLOW, animate=True)

        self.wait(0.5)
        # The symmetric axis-swap matrix has the default zero path arc.
        self.apply_matrix([[0, 1], [1, 0]])
        self.wait(0.5)
        self.apply_matrix([[0, 1], [1, 0]])
        self.wait(0.5)
