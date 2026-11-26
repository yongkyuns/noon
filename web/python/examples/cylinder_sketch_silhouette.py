"""Tilt from the side of a sketched cylinder to an almost end-on view."""
from noon import *


class CylinderSketchSilhouette(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=1.54, theta=-1.12, focal_distance=10, zoom=1.65)
        cylinder = Cylinder(
            height=2.5, fill_color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xACEAD7), roughness=0.85, seed=27), stroke_width=5,
        )
        self.add_world_mobjects(cylinder)

        self.wait(0.25)
        self.move_camera(phi=0.035, theta=-1.12, run_time=2.5, rate_func=linear)
        self.wait(0.25)
