from noon import *


class ArrowVectorFieldStatic(Scene):
    def construct(self):
        field = ArrowVectorField(
            lambda point: point[0] * UP - point[1] * RIGHT,
        )
        self.add(field)
        self.wait(0.2)
