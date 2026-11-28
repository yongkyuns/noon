"""Show a tilted Line3D sketch from the side and along its axis."""
from math import atan2, pi

from noon import *


class Line3DSketchSilhouette(ThreeDScene):
    def construct(self):
        end_phi = atan2(0.6, 0.8)
        self.set_camera_orientation(phi=end_phi + pi / 2, theta=0, focal_distance=10, zoom=1.65)
        tube = Line3D(
            (-0.9, 0, -1.2), (0.9, 0, 1.2), thickness=0.62, color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xACEAD7), roughness=0.85, seed=27), stroke_width=5,
        )
        self.add_world_mobjects(tube)

        self.wait(0.25)
        self.move_camera(phi=end_phi, theta=0, run_time=2.5, rate_func=linear)
        self.wait(0.25)
