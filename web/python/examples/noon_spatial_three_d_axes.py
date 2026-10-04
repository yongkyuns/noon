from noon import *


class SpatialThreeDAxes(ThreeDScene):
    """Default pinned axis ranges/lengths/tips with a Rust-coordinate point."""

    def __init__(self):
        super().__init__(near=0.1, far=100)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        axes = ThreeDAxes()
        coordinate = axes.c2p(2, -1, 1.5)
        # Dot remains a planar circle. Lift its Rust-derived coordinate into the
        # shared world domain with an explicit native depth translation.
        point = Dot(coordinate[:2], radius=0.16, color=RED)
        self.shift_world(point, (0, 0, coordinate[2]))
        self.add_world_mobjects(axes, point)
        await self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
