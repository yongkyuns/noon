import math

from noon import *


class SpatialPrimitives(ThreeDScene):
    def __init__(self):
        super().__init__(near=0.1, far=30)

    async def construct(self):
        self.set_camera_orientation(
            phi=0, theta=-PI / 2, gamma=0, focal_distance=5,
            zoom=4 / (5 * math.tan(0.5)), frame_center=(0, 0, 0),
        )
        line = Line3D(
            start=(-2.0, -0.8, 0.0), end=(-0.2, -0.8, 0.0),
            thickness=0.18, color=RED, resolution=16,
            checkerboard_colors=False, stroke_width=0, shade_in_3d=False,
        )
        triangle = Mesh3D.polyhedron(
            [(0.45, -1.0, 0.0), (2.25, -1.0, 0.0), (1.35, 1.0, 0.25)],
            [(0, 1, 2)], color=BLUE,
        )
        self.add(line, triangle)
        await self.play(
            WorldTransformTo(line, translation=(0, 0, 0.25)),
            WorldTransformTo(
                triangle,
                rotation=(0.9800665778412416, 0, 0.19866933079506122, 0),
            ),
            run_time=1, rate_func=linear,
        )
