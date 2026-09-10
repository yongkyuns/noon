from noon import Arrow, DoubleArrow, Scene, Vector


class OrdinaryArrowGeometry(Scene):
    def construct(self):
        arrow = Arrow((-3.0, 1.5), (-0.5, 1.5), buff=0.2)
        vector = Vector((2.0, 1.0)).shift((0.0, -0.5))
        double = DoubleArrow((-2.5, -1.5), (2.5, -1.5), buff=0.15)

        self.add(arrow, vector, double)
