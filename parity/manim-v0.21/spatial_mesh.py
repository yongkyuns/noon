"""Pinned ManimCE oracle for Noon's typed shared spatial-mesh fixture."""

import numpy as np
from manim import *


class SpatialMeshDepthOracle(ThreeDScene):
    """Opaque shared triangle, moving camera, and a perspective depth crossing."""

    def construct(self):
        self.set_camera_orientation(
            phi=0,
            theta=-PI / 2,
            focal_distance=5,
            zoom=4 / (5 * np.tan(0.5)),
            frame_center=ORIGIN,
        )
        self.camera.should_apply_shading = False
        triangle = [
            np.array([-1.0, -1.0, 0.0]),
            np.array([1.0, -1.0, 0.0]),
            np.array([0.0, 1.0, 0.0]),
        ]
        self.red_mesh = Polygon(
            *(point + np.array([0.0, 0.0, 1.0]) for point in triangle),
            fill_color=RED,
            fill_opacity=1.0,
            stroke_width=0,
            shade_in_3d=True,
        )
        self.blue_mesh = Polygon(
            *triangle,
            fill_color=BLUE,
            fill_opacity=1.0,
            stroke_width=0,
            shade_in_3d=True,
        )
        # The blue rear mesh is intentionally added last. Its visibility must be
        # decided by camera-space depth after the red mesh passes behind it.
        self.add(self.red_mesh, self.blue_mesh)
        self.move_camera(
            frame_center=RIGHT * 0.25,
            run_time=2.0,
            rate_func=linear,
            added_anims=[self.red_mesh.animate.shift(2.0 * IN)],
        )

    def noon_oracle_state(self):
        """Numeric checkpoints consumed by the pinned raster qualification."""
        return {
            "camera": {
                "phi": float(self.camera.get_phi()),
                "theta": float(self.camera.get_theta()),
                "focal_distance": float(self.camera.get_focal_distance()),
                "zoom": float(self.camera.get_zoom()),
                "frame_center": np.asarray(self.camera.frame_center, dtype=float).tolist(),
            },
            "red_center": np.asarray(self.red_mesh.get_center(), dtype=float).tolist(),
            "blue_center": np.asarray(self.blue_mesh.get_center(), dtype=float).tolist(),
        }
