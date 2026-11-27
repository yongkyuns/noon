"""Show the torus hole closing during a face-on to edge-on camera move."""
from noon import *


class TorusSketchSilhouette(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0, theta=-1.12, focal_distance=10, zoom=1.65)
        torus = Torus(
            major_radius=1.5, minor_radius=0.62, fill_color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xACEAD7), roughness=0.85, seed=27), stroke_width=5,
        )
        self.add_world_mobjects(torus)

        self.wait(0.25)
        self.move_camera(phi=1.5707963267948966, theta=-1.12, run_time=2.5, rate_func=linear)
        self.wait(0.25)
