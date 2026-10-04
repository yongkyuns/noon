from manim import *


class SpatialCameraLabels(ThreeDScene):
    """Finite-camera world, fixed-orientation family, and fixed-frame labels."""

    @staticmethod
    def fit_label(mobject, width, height, center):
        # Align geometry and placement while retaining the pinned Cairo glyphs.
        mobject.stretch_to_fit_width(width)
        mobject.stretch_to_fit_height(height)
        mobject.move_to(center)

    def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
        )
        background = Square(side_length=6, color=ManimColor((0.08, 0.12, 0.22)))
        background.set_fill(ManimColor((0.08, 0.12, 0.22)), opacity=1)
        background.set_stroke(width=0)
        world_label = Text("World label", color=RED)
        formula = MathTex(r"\frac{x}{2}", color=WHITE)
        fixed_left = Text("Fixed label", color=RED)
        fixed_right = Text("Anchor label", color=RED)
        fixed_left.set_opacity(0.5)
        fixed_right.set_opacity(0.5)
        fixed_family = VGroup(fixed_left, fixed_right)
        hud = Text("Fixed frame", color=YELLOW)

        self.fit_label(world_label, 1.8, 0.28, (-3.2, 2.6, 0.1))
        self.fit_label(formula, 0.8, 0.55, (2.2, 2.6, 0.1))
        self.fit_label(fixed_left, 1.2, 0.24, (-1, -2, 0.3))
        self.fit_label(fixed_right, 1.2, 0.24, (1, -2, -0.3))
        self.fit_label(hud, 1.1, 0.22, (-3.4, -3.4, 0))
        self.world_label = world_label
        self.formula = formula
        self.fixed_left = fixed_left
        self.fixed_right = fixed_right
        self.fixed_family = fixed_family
        self.hud = hud

        self.add(background, world_label, formula)
        self.add_fixed_orientation_mobjects(fixed_family)
        self.add_fixed_in_frame_mobjects(hud)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )

    def noon_oracle_state(self):
        def label_state(mobject):
            return {
                "center": np.asarray(mobject.get_center(), dtype=float).tolist(),
                "width": float(mobject.get_width()),
                "height": float(mobject.get_height()),
            }

        return {
            "camera": {
                "phi": float(self.camera.get_phi()),
                "theta": float(self.camera.get_theta()),
                "gamma": float(self.camera.get_gamma()),
                "focal_distance": float(self.camera.get_focal_distance()),
                "zoom": float(self.camera.get_zoom()),
                "frame_center": np.asarray(self.camera.frame_center, dtype=float).tolist(),
            },
            "labels": {
                "world": label_state(self.world_label),
                "formula": label_state(self.formula),
                "fixed_left": label_state(self.fixed_left),
                "fixed_right": label_state(self.fixed_right),
                "fixed_family_center": np.asarray(self.fixed_family.get_center(), dtype=float).tolist(),
                "hud": label_state(self.hud),
            },
        }
