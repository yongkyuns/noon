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
