from manim import *


class UnshadedThreeDAxes(ThreeDAxes):
    # Cairo's segmented pieces and directional sheen are not in this native
    # renderer slice. Keep the pinned default axes/ticks/tips, omitting only
    # those renderer-specific decorations for the representative geometry oracle.
    def _add_3d_pieces(self):
        pass

    def _set_axis_shading(self):
        pass


class SpatialThreeDAxes(ThreeDScene):
    def construct(self):
        self.set_camera_orientation(
            phi=0.6, theta=-1.2, gamma=0, focal_distance=5, zoom=1,
            frame_center=(0, 0, 0),
        )
        axes = UnshadedThreeDAxes(
            x_axis_config={"color": RED, "include_tip": False, "stroke_width": 4},
            y_axis_config={"color": GREEN, "tick_size": 0.15},
            z_axis_config={"color": BLUE},
        )
        labels = axes.get_axis_labels(
            Text("x", font="DejaVu Sans Mono"),
            Text("y", font="DejaVu Sans Mono"),
            Text("z", font="DejaVu Sans Mono"),
        )
        point = Dot(axes.c2p(2, -1, 1.5), radius=0.16, color=RED)
        self.add(axes, point, labels)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
