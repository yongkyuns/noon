from noon import *

class OrdinaryPathRefinement(Scene):
    def construct(self):
        point = lambda x, y: RIGHT * x + UP * y
        original = VMobject(color=BLUE).start_new_path(point(-3, 1))
        original.add_cubic_bezier_curve_to(point(-3, 3), point(0, 3), point(0, 1))
        original.add_line_to(point(3, 1))
        refined = original.copy().insert_n_curves(3).shift(DOWN * 3).set_stroke(YELLOW)
        assert refined.get_num_curves() == 5
        assert len(refined.get_anchors_and_handles()[0]) == 5
        self.add(original, refined)
        for anchor in list(refined.get_start_anchors()) + [refined.get_end()]:
            self.add(Dot(anchor, radius=0.06, color=RED))
        self.wait(0.2)
