from noon import *


class OrdinaryPointMatching(Scene):
    def construct(self):
        source = Square(side_length=1, color="#58c4dd")
        target = Arc(radius=1.4, start_angle=-0.4, angle=4.7, num_components=9).rotate(0.2)
        source.match_points(target).shift(LEFT * 2.5)
        line = Line(LEFT, RIGHT)
        ellipse = Ellipse(width=2, height=1).rotate(0.6)
        line.match_points(ellipse).shift(RIGHT * 2.5)
        self.add(source, line)
        self.wait(0.2)
