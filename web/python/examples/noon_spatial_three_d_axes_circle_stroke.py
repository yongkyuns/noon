from noon import *


class SpatialCircleScreenStroke(ThreeDScene):
    """Tilted, nonuniform Circle screen stroke under camera motion."""

    def __init__(self):
        super().__init__(near=0.1, far=100)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        circle = Circle(radius=0.65, color=WHITE)
        circle.set_fill(opacity=0)
        circle.set_stroke(WHITE, width=4)
        self.add_world_mobjects(circle)
        self.set_world_transform(
            circle,
            translation=(-1.3, 1.35, 0.0),
            rotation=(0.9747941070689433, 0.0, 0.22310636213174545, 0.0),
            scale=(1.1, 0.7, 1.0),
        )
        await self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
