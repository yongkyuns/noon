from noon import *


class OrdinaryPathEditing(Scene):
    def construct(self):
        path = VMobject(color="#58c4dd").set_points_as_corners([(-3, -1, 0), (-2, 1, 0), (-1, -1, 0)])
        original = path.copy().shift(UP * 2.5)
        self.add(path, original)
        polygon = Square(side_length=1).shift(RIGHT * 7 + UP * 3).rotate(0.6)
        polygon.set_fill(YELLOW, opacity=0.3)
        polygon.set_points_as_corners([(1, -1, 0), (3, -1, 0), (2, 1, 0), (1, -1, 0)])
        self.add(polygon)
        path.set_points_as_corners([(-3, -1, 0), (-2, 0, 0), (-1, -1, 0)])
        self.wait(0.2)
