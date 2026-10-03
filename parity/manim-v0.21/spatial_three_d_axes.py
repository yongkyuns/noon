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
        axes = UnshadedThreeDAxes()  # pinned default ranges, lengths, ticks, and tips
        point = Dot(axes.c2p(2, -1, 1.5), radius=0.16, color=RED)
        self.add(axes, point)
        self.move_camera(
            phi=0.8, theta=-0.1, gamma=0.2, zoom=1.1,
            frame_center=(0.3, 0, 0), run_time=1, rate_func=linear,
        )
