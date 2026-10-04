import math

from noon import *


class ThreeDCameraIllusionRotation(ThreeDScene):
    async def construct(self):
        axes = ThreeDAxes()
        # Explicit Noon World-path stroke profile; Cairo raster parity is separate.
        circle = Circle(stroke_width_mode="scale_with_object")
        self.set_camera_orientation(phi=75 * DEGREES, theta=30 * DEGREES)
        self.add(circle,axes)
        self.begin_3dillusion_camera_rotation(rate=2)
        await self.wait(PI/2)
        self.stop_3dillusion_camera_rotation()
