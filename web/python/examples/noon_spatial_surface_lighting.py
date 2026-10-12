from noon import *


class SpatialSurfaceLighting(SpatialScene):
    def __init__(self):
        super().__init__(far=30)

    async def construct(self):
        surface = Mesh3D.parametric(
            lambda u, v: (u, v, 0.25 * u * v),
            u_range=(-1.5, 1.5),
            v_range=(-1.5, 1.5),
            resolution=(8, 8),
            color=Color(0.2, 0.55, 0.85),
            point_lit=True,
        )
        light = self.point_light(position=(4, -3, 6), color=Color(1, 1, 1), intensity=1)
        self.add(surface, light)
        await self.play(
            WorldTransformTo(light, translation=(-3, 3, 4)),
            run_time=1,
            rate_func=linear,
        )
