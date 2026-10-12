"""Representative retained Matrix against pinned ManimCE."""
from manim import *


class OrdinaryMatrix(Scene):
    def construct(self):
        self.add(Matrix([["1", "2"], ["x", "y"]]))
        self.wait(0.2)
