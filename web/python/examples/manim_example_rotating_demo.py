from noon import *


class RotatingDemo(Scene):
    def construct(self):
        circle = Circle(radius=1, color=BLUE)
        line = Line(start=ORIGIN, end=RIGHT)
        arrow = Arrow(start=ORIGIN, end=RIGHT, buff=0, color=GOLD)
        vg = VGroup(circle,line,arrow)
        self.add(vg)
        anim_kw = {"about_point": arrow.get_start(), "run_time": 1}
        self.play(Rotating(arrow, 180*DEGREES, **anim_kw))
        self.play(Rotating(arrow, PI, **anim_kw))
        self.play(Rotating(vg, PI, about_point=RIGHT))
        self.play(Rotating(vg, PI, axis=UP, about_point=ORIGIN))
        self.play(Rotating(vg, PI, axis=RIGHT, about_edge=UP))
        self.play(vg.animate.move_to(ORIGIN))
