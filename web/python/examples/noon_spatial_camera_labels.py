from noon import *


class SpatialCameraLabels(ThreeDScene):
    def __init__(self):
        super().__init__(near=0.1, far=100)

    async def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        background = Square(6, color=Color(0.08, 0.12, 0.22))
        background.set_fill(Color(0.08, 0.12, 0.22), opacity=1)
        background.set_stroke(width=0)
        world_label = Typst("#text(fill: red)[World label]")
        formula = MathTypst("frac(x, 2)")
        fixed_left = Typst("#text(fill: red)[Fixed label]").set_opacity(0.5)
        fixed_right = Typst("#text(fill: red)[Anchor label]").set_opacity(0.5)
        fixed_family = VGroup(fixed_left, fixed_right)
        hud = Typst("#text(fill: yellow)[Fixed frame]").move_to((-3.4, -3.4, 0))

        self.shift_world(background, (0, 0, 0))
        self.shift_world(world_label, (-3.2, 2.6, 0.1))
        self.shift_world(formula, (2.2, 2.6, 0.1))
        self.shift_world(fixed_left, (-1, -2, 0.3))
        self.shift_world(fixed_right, (1, -2, -0.3))
        self.add_world_mobjects(background, world_label, formula)
        self.add_fixed_orientation_mobjects(fixed_family)
        self.add_fixed_in_frame_mobjects(hud)
        await self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
