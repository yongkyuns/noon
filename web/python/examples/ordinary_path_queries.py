from noon import *


class OrdinaryPathQueries(Scene):
    def construct(self):
        rectangle = Rectangle(width=2, height=1, color=WHITE)
        rectangle.stretch_to_fit_width(4).rotate(0.25).shift(LEFT * 2.5)
        ellipse = Circle(radius=0.8, color=WHITE)
        ellipse.stretch_to_fit_width(2.4).shift(RIGHT * 2.5)
        for shape in (rectangle, ellipse):
            self.add(shape)
            assert shape.get_arc_length() > 0
            for alpha in (0.125, 0.375, 0.625, 0.875):
                self.add(Dot(shape.point_from_proportion(alpha), color="#ffcc44"))
        target = rectangle.copy().stretch(1.2, 0)
        start, end = rectangle.get_start(), target.get_start()
        animation = self.declare_live_transform_to(rectangle, target, run_time=1, rate_func=linear)
        live = self.live_execution()
        finish = live.play(animation)
        live.advance_to(0.5)
        midpoint = rectangle.get_start()
        assert abs(midpoint[0] - (start[0] + end[0]) * 0.5) < 2e-6
        assert abs(midpoint[1] - (start[1] + end[1]) * 0.5) < 2e-6
        live.advance_to(finish)
        live.complete()
        wait_end = live.wait(0.2)
        live.advance_to(wait_end)
        live.complete()
        self.play(Create(ellipse), run_time=1, rate_func=linear)
        observed = self.live_execution()
        observed.evaluate(1.7)
        endpoint = ellipse.get_end()
        assert abs(endpoint[0] - 1.3) < 2e-6 and abs(endpoint[1]) < 2e-6
        observed.evaluate(2.2)
