from noon import *


class SpatialSceneShowcase(ThreeDScene):
    """A lit surface, one moving point light, and readable spatial labels."""

    def __init__(self):
        super().__init__(near=0.1, far=60)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.95, theta=-1.15, gamma=0.05, focal_distance=6,
            zoom=1, frame_center=(0, 0, 0),
        )

        surface = Mesh3D.parametric(
            lambda u, v: (u, v, 0.28 * (u * u - v * v)),
            u_range=(-1.7, 1.7),
            v_range=(-1.25, 1.25),
            resolution=(20, 16),
            color=Color(0.12, 0.58, 0.86),
            point_lit=True,
        )
        marker = Mesh3D.sphere(
            radius=0.22, resolution=(16, 8), color=Color(1.0, 0.48, 0.16),
        )
        marker.shift((0, 0, 0.75))
        light = self.point_light(
            position=(3.8, -3.2, 5.0), color=Color(1.0, 0.91, 0.72), intensity=1.25,
        )

        title = Typst("Point light on a parametric surface")
        title.move_to((0, 3.45, 0))
        anchor_label = Typst("World-anchored label")
        anchor_label.move_to((0, -2.9, 0))
        self.add_world_mobjects(surface, marker, light)
        self.add_fixed_orientation_mobjects(anchor_label)
        self.add_fixed_in_frame_mobjects(title)

        # Beat 1: reveal the surface's changing highlights by moving its light and view.
        await self.move_camera(
            phi=1.12, theta=-0.65, gamma=0.08, focal_distance=6, zoom=1.05,
            frame_center=(0, 0, 0),
            added_anims=(WorldTransformTo(light, translation=(-3.2, 2.8, 4.2)),),
            run_time=1.8, rate_func=linear,
        )
        await self.wait(0.45)

        # Beat 2: lift the solid marker above the saddle while the light holds.
        await self.play(
            WorldTransformTo(marker, translation=(0.55, 0.15, 1.0),
                             rotation=(1, 0, 0, 0), scale=(1, 1, 1)),
            run_time=1.5, rate_func=smooth,
        )
        await self.wait(0.4)

        # Beat 3: move both the light and camera to show the surface from the far side.
        await self.move_camera(
            phi=0.78, theta=0.55, gamma=-0.12, focal_distance=6, zoom=1.08,
            frame_center=(0.1, 0, 0),
            added_anims=(WorldTransformTo(light, translation=(3.0, 2.6, 4.8)),),
            run_time=1.8, rate_func=linear,
        )
        await self.wait(2.05)
