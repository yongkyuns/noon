from noon import *


def slide(scene, square, expected_x):
    scene.play(square.animate.shift(RIGHT), run_time=0.25, rate_func=linear)
    assert abs(square.get_center()[0] - expected_x) < 1e-5


class OrdinaryBlockingAuthoring(Scene):
    def construct(self):
        square = Square(side_length=1).set_fill(BLUE, opacity=1)
        line = Line(ORIGIN, LEFT).set_color(YELLOW)

        def turn(target, dt):
            target.rotate_about_origin(dt)

        line.add_updater(turn)
        self.add(square, line)
        for index in range(1, 3):
            slide(self, square, index)
        self.wait(0.25)
        line.remove_updater(turn)
        square.set_fill(ORANGE, opacity=1)
        self.wait(0.25)
