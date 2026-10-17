from manim import *


class AngleConstructors(Scene):
    def construct(self):
        for index in range(6):
            x = (index % 3) * 4 - 4
            y = 1.7 if index < 3 else -1.7
            end = 0.3 if index == 4 else 1.2
            first = Line([-1.2, 0, 0], [end, 0, 0], color=YELLOW).shift([x, y, 0])
            second = (Line([-1.2, 0.5, 0], [1.2, 0.5, 0], color=YELLOW) if index == 5
                      else Line([0, -1.2, 0], [0, 1.2, 0], color=YELLOW)).shift([x, y, 0])
            if index == 1:
                angle = Angle(first, second, radius=0.7, other_angle=True)
            elif index == 2:
                angle = Angle(first, second, radius=0.5, quadrant=(-1, 1))
            elif index == 3:
                angle = RightAngle(first, second)
            else:
                angle = Angle(first, second)
            if index != 3:
                assert abs(angle.get_value(degrees=True) - [90, -270, 270, 0, 90, 0][index]) < 1e-9
            angle.angle_value = (index + 1) / 8
            assert angle.get_value() == (index + 1) / 8
            self.add(first, second, angle)
        self.wait(0.2)
