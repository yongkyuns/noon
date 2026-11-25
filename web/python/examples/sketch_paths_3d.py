"""Draw two sketch strokes during a camera orbit with an opaque foreground panel."""
from noon import *


class SpatialSketchPaths(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(phi=0.32, theta=-1.85, focal_distance=10, zoom=1.5)

        panel = Rectangle(
            width=2.65, height=1.85, fill_color=BLUE_E, fill_opacity=1,
            stroke=SketchStroke(TEAL, roughness=1.4, seed=7), stroke_width=4,
        )
        bezier = VectorPath().move_to((-2.25, -0.58)).cubic_to(
            (-0.65, -1.8), (0.7, 1.2), (2.55, -0.4),
        )
        curve = Path(
            bezier, fill_opacity=0,
            stroke=SketchStroke(YELLOW, roughness=1.3, seed=17), stroke_width=5,
        )
        occluder = Rectangle(
            width=1.15, height=1.45, fill_color=RED_E, fill_opacity=1, stroke_width=0,
        )
        self.set_world_transform(panel, translation=(-0.9, 0.5, 0),
                                 rotation=(0.9659258263, 0.2588190451, 0, 0))
        self.set_world_transform(occluder, translation=(0.4, -0.18, 1.15))
        self.set_world_transform(curve)
        self.add_world_mobjects(occluder)

        self.move_camera(
            phi=0.85, theta=-0.6, added_anims=(Create(panel), Create(curve)),
            run_time=2.6, rate_func=linear,
        )
        self.wait(0.4)
