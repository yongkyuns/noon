"""Tilt from the side of a sketched cone to a base-on view."""
from noon import *


class ConeSketchSilhouette(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=1.5707963267948966, theta=-1.12, focal_distance=10, zoom=1.65)
        cone = Cone(
            height=2.5, show_base=True, fill_color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xACEAD7), roughness=0.85, seed=27), stroke_width=5,
        )
        cone.shift((0, 0, 1.25))
        self.add_world_mobjects(cone)

        self.wait(0.25)
        self.move_camera(phi=3.141592653589793, theta=-1.12, run_time=2.5, rate_func=linear)
        self.wait(0.25)
