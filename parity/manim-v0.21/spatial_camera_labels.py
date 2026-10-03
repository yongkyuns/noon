from manim import *


class SpatialCameraLabels(ThreeDScene):
    """Finite-camera world, fixed-orientation family, and fixed-frame labels."""

    def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
        )
        background = Square(side_length=6, color=ManimColor((0.08, 0.12, 0.22)))
        background.set_fill(ManimColor((0.08, 0.12, 0.22)), opacity=1)
        background.set_stroke(width=0)
        world_label = Text("World label", color=RED).move_to((-3.2, 2.6, 0.1))
        formula = MathTex(r"\frac{x}{2}", color=WHITE).scale(0.65).move_to((2.2, 2.6, 0.1))
        fixed_left = Text("Fixed label", color=RED).scale(0.55).move_to((-1, -2, 0.3))
        fixed_right = Text("Anchor label", color=RED).scale(0.55).move_to((1, -2, -0.3))
        fixed_left.set_opacity(0.5)
        fixed_right.set_opacity(0.5)
        fixed_family = VGroup(fixed_left, fixed_right)
        hud = Text("Fixed frame", color=YELLOW).scale(0.45).to_corner(DL)

        self.add(background, world_label, formula)
        self.add_fixed_orientation_mobjects(fixed_family)
        self.add_fixed_in_frame_mobjects(hud)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
