from noon import *

class OrdinaryPathSelection(Scene):
    def construct(self):
        point = lambda x, y: RIGHT * x + UP * y
        curve = VMobject(color=BLUE).start_new_path(point(-3, -1))
        curve.add_cubic_bezier_curve_to(point(-3, 2), point(0, 2), point(0, -1))
        curve.start_new_path(point(1, -1)).add_line_to(point(3, 1))
        selected = curve.copy().pointwise_become_partial(curve, 0.2, 0.8)
        selected.shift(DOWN * 2).set_stroke(YELLOW)
        self.add(curve, selected)
        selected.reverse_direction()
        self.wait(0.2)
