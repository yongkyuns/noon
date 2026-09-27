"""The same retained Matrix scene as the native/direct-WASM example."""
from noon import *


class OrdinaryMatrix(Scene):
    async def construct(self):
        await prepare_latex()
        self.add(Matrix([["1", "2"], ["x", "y"]]))
