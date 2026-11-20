"""Animate a tracker-driven dot beside an independently moving group."""
from noon import *


class OrdinaryMixedScalarComposition(Scene):
    def construct(self):
        circle = Circle(radius=0.3).set_fill(BLUE, opacity=0.7)
        square = Square(side_length=1).set_fill(PINK, opacity=0.7).shift(DOWN)
        markers = VGroup(*(Circle(radius=0.16, fill=YELLOW, fill_opacity=1, stroke=None)
                           .shift(x * RIGHT + 1.5 * DOWN) for x in (-1.3, 1.3)))
        left, right = markers.submobjects
        self.add(circle, square, markers)
        tracker = self.value_tracker(0)
        self.bind_position(circle, tracker, direction=RIGHT, offset=LEFT * 2 + UP)
        self.play(Succession(
            Wait(0.5),
            AnimationGroup(
                tracker.animate(run_time=2, rate_func=linear).set_value(4),
                Rotate(square, PI, run_time=2, rate_func=linear),
                markers.animate(run_time=2, rate_func=linear).shift(DOWN),
                rate_func=smooth,
            ),
            rate_func=linear,
        ))
        assert abs(tracker.get_value() - 4) < 1e-6
        assert abs(circle.get_center()[0] - 2) < 1e-6
        assert markers.submobjects[0] is left and markers.submobjects[1] is right
        assert all(abs(marker.get_y() + 2.5) < 1e-6 for marker in (left, right))
        self.wait(0.25)
        tracker.set_value(3)
        self.wait(0.25)
        assert abs(circle.get_center()[0] - 1) < 1e-6
