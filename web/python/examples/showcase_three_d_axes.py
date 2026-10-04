"""Place a marker with ThreeDAxes coordinates while the camera moves."""
from noon import *


class ThreeDimensionalCoordinates(ThreeDScene):
    def __init__(self):
        super().__init__(near=0.1, far=100)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.95, theta=-1.15, gamma=0, focal_distance=14,
            zoom=0.65, frame_center=(0, 0, 0),
        )
        axes = ThreeDAxes()
        marker = Dot3D(
            point=axes.c2p(2, -1, 1.5), radius=0.13, color=RED,
            stroke_width=0, shade_in_3d=False,
        )
        caption = Typst("Coordinates: (2, -1, 1.5)")
        caption.move_to((0, 3.45, 0))
        self.add_world_mobjects(axes, marker)
        self.add_fixed_in_frame_mobjects(caption)

        await self.wait(0.6)
        await self.move_camera(
            phi=1.12, theta=-0.7, gamma=0.04, focal_distance=14,
            zoom=0.68, frame_center=(0, 0, 0),
            run_time=1.6, rate_func=linear,
        )
        await self.wait(0.5)
        await self.move_camera(
            phi=0.82, theta=0.35, gamma=-0.08, focal_distance=14,
            zoom=0.68, frame_center=(0.15, 0, 0),
            run_time=1.6, rate_func=linear,
        )
        await self.wait(1.7)
