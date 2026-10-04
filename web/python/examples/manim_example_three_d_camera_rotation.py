import math

from noon import *


class ThreeDCameraRotation(ThreeDScene):
    async def construct(self):
        axes = ThreeDAxes()
        # Explicit Noon World-path stroke profile; Cairo raster parity is separate.
        circle = Circle(stroke_width_mode="scale_with_object")
        self.set_camera_orientation(phi=75 * DEGREES, theta=30 * DEGREES)
        self.add(circle,axes)
        self.begin_ambient_camera_rotation(rate=0.1)
        await self.wait()
        self.stop_ambient_camera_rotation()
        await self.move_camera(phi=75 * DEGREES, theta=30 * DEGREES)
        await self.wait()
