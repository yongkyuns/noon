from noon import *


class LiveMaskedPlacement(Scene):
    def construct(self):
        source = Rectangle(width=4, height=2).move_to((2, -1, 0))
        reference = Rectangle(width=2, height=4).move_to((-3, 3, 0))
        self.add(source, reference)
        self.wait(0.25)
        self.play(source.animate.move_to((99, 5, 0), aligned_edge=UP,
                                        coor_mask=(0, 1, 0)),
                  run_time=0.5, rate_func=linear)
        assert abs(source.get_x() - 2) < 1e-6
        assert abs(source.get_top()[1] - 5) < 1e-6
        self.play(source.animate.move_to(reference, aligned_edge=UP,
                                        coor_mask=(0.5, 1, 0)),
                  run_time=0.5, rate_func=linear)
        assert abs(source.get_x() + 0.5) < 1e-6
        assert abs(source.get_top()[1] - reference.get_top()[1]) < 1e-6
