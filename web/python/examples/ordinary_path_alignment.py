from noon import *

class OrdinaryPathAlignment(Scene):
    def construct(self):
        point = lambda x, y: RIGHT * x + UP * y
        line = Line(point(-3, -1), point(-1, -1))
        wave = VMobject().start_new_path(point(1, -1))
        wave.add_cubic_bezier_curve_to(point(1, 2), point(3, 2), point(3, -1))
        wave.insert_n_curves(4)
        line.align_points(wave)
        assert line.get_num_curves() == wave.get_num_curves() == 5
        self.add(line.set_color(BLUE), wave.set_color(YELLOW))
        self.wait(0.2)
