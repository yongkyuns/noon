"""The same retained Matrix scene as the native/direct-WASM example."""
from noon import *


class OrdinaryMatrix(Scene):
    requires_latex = True

    def construct(self):
        self.add(Matrix([["1", "2"], ["x", "y"]]))
