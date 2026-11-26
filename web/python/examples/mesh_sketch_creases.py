"""Trace a beveled cube's creases during a camera orbit."""
from noon import *


class MeshSketchCreases(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0.64, theta=-1.12, focal_distance=10, zoom=1.65)
        vertices = (
            (-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1),
            (-1, -1, 0.65), (1, -1, 0.65), (1, 1, 0.65), (-1, 1, 0.65),
            (-0.65, -0.65, 1), (0.65, -0.65, 1), (0.65, 0.65, 1), (-0.65, 0.65, 1),
        )
        triangles = (
            (0, 3, 2), (0, 2, 1), (0, 1, 5), (0, 5, 4), (1, 2, 6), (1, 6, 5),
            (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7), (4, 5, 9), (4, 9, 8),
            (5, 6, 10), (5, 10, 9), (6, 7, 11), (6, 11, 10), (7, 4, 8), (7, 8, 11),
            (8, 9, 10), (8, 10, 11),
        )
        cube = Mesh3D.polyhedron(
            vertices, triangles, color=BLUE_E,
            stroke=SketchStroke(color_from_hex(0xF7D996), roughness=1.1, seed=27), stroke_width=5,
        )
        self.add_world_mobjects(cube)
        self.wait(0.25)
        self.move_camera(phi=1.1, theta=0.67, run_time=2.5, rate_func=linear)
        self.wait(0.25)
