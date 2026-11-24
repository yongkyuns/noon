"""The same retained Matrix scene as the native/direct-WASM example."""
from noon import *


class OrdinaryMatrix(Scene):
    def construct(self):
        self.add(Matrix([["1", "2"], ["x", "y"]]))
