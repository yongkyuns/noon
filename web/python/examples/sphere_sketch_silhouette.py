"""Orbit a sphere with a seeded, camera-facing sketch silhouette."""
from noon import *


class SphereSketchSilhouette(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0.64, theta=-1.12, focal_distance=10, zoom=1.65)
        sphere = Sphere(
            center=(0.8, -0.45, 0.1), radius=1.35, fill_color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xACEAD7), roughness=1.15, seed=27), stroke_width=5,
        )
        self.add_world_mobjects(sphere)

        self.wait(0.25)
        self.move_camera(phi=1.1, theta=0.67, run_time=2.5, rate_func=linear)
        self.wait(0.25)
