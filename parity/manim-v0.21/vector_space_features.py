"""Pinned Manim oracle for LTS coordinates and transformable ghost vectors."""

from manim import *


class VectorSpaceLTSFeatures(LinearTransformationScene):
    def __init__(self, **kwargs):
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
            **kwargs,
        )

    def construct(self):
        square = Square(side_length=0.5).move_to([0.5, 0.5, 0.0])
        self.add_transformable_mobject(square)
        vector = Vector((0.5, 0.25), color=YELLOW)
        self.add_vector(vector, animate=False)
        self.wait(0.25)
        self.apply_matrix([[0.0, 1.0], [1.0, 0.0]], run_time=0.5)
        self.wait(0.25)
        self.apply_matrix([[2.0, 0.0], [0.0, 1.0]], run_time=0.5)
