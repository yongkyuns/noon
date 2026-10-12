from noon import *


class RetainedZoom(ZoomedScene):
    def construct(self):
        context = Circle(radius=0.85, color=BLUE, fill_color=BLUE, fill_opacity=0.18, stroke_width=3.5)
        focus = Dot((0.45, 0.15, 0), radius=0.22, color=YELLOW)
        self.add(context, focus)
        self.activate_zooming(animate=False)
        self.play(
            self.zoomed_camera.frame.animate.shift((0.9, 0.3, 0)),
            run_time=1,
            rate_func=linear,
        )
