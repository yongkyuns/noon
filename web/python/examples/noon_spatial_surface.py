from noon import *


class SpatialSurface(SpatialScene):
    def __init__(self):
        super().__init__(far=30)

    async def construct(self):
        surface = Surface(
            lambda u, v: (u, v, 0.25 * u * v),
            u_range=(-1.5, 1.5),
            v_range=(-1.5, 1.5),
            resolution=(8, 8),
            checkerboard_colors=False,
            fill_color=Color(0.2, 0.55, 0.85),
            fill_opacity=1,
            stroke_width=0,
            shade_in_3d=False,
        )
        self.add(surface)
        await self.play(WorldTransformTo(
            surface, rotation=(0.955336489125606, 0, 0, 0.29552020666134)
        ), run_time=1, rate_func=linear)


class CairoSpatialSurface(SpatialScene):
    def __init__(self):
        super().__init__(far=30)

    def construct(self):
        surface = Surface(
            lambda u, v: (u, v, 0.35 * (u * u + v * v)),
            u_range=(-1, 1),
            v_range=(-1, 1),
            resolution=(8, 8),
        )
        self.add(surface)


class CairoSphereTorus(SpatialScene):
    """Pinned default shaded Sphere/Torus families with a moving camera."""

    def __init__(self):
        super().__init__(near=0.1, far=100)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        sphere = Sphere(center=(-1.2, 0, 0), radius=0.6)
        torus = Torus(major_radius=0.6, minor_radius=0.2).shift((1.2, 0, 0))
        self.add_world_mobjects(sphere, torus)
        await self.play(
            CameraProfileTo(
                self.camera,
                (0.8, -0.1, 0.2, 5.0, 1.1, 8.0, 0.3, 0.0, 0.0),
            ),
            run_time=1,
            rate_func=linear,
        )


class CairoConeBodies(SpatialScene):
    """Default and tilted partial Cone bodies with pinned Surface defaults."""

    def __init__(self):
        super().__init__(far=30)

    def construct(self):
        default_cone = Cone().shift((-1.25, 0, 0))
        tilted_partial_cone = Cone(
            base_radius=0.8, height=1.4, direction=(1, 2, 2), u_min=0.2,
        ).shift((1.25, 0, 0))
        self.add(default_cone, tilted_partial_cone)
