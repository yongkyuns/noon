"""Pinned ManimCE 0.21 VectorScene/LTS matrix oracle for Noon's retained scene."""

import numpy as np
from manim import *


class VectorSpaceLTS(LinearTransformationScene):
    def construct(self):
        self.test_vector = Vector((2.0, 1.0), color=YELLOW)
        self.add_vector(self.test_vector, animate=True)
        self.apply_matrix([[0.0, 1.0], [1.0, 0.0]])

    def noon_oracle_state(self):
        def endpoints(vector):
            points = vector.get_all_points()
            return {
                "start": np.asarray(vector.get_start(), dtype=float).tolist(),
                "end": np.asarray(vector.get_end(), dtype=float).tolist(),
                "color": vector.get_color().to_rgb().tolist(),
                "bounds": {
                    "min": points[:, :2].min(axis=0).tolist(),
                    "max": points[:, :2].max(axis=0).tolist(),
                },
            }

        return {
            "basis_i": endpoints(self.i_hat),
            "basis_j": endpoints(self.j_hat),
            "moving_vector": endpoints(self.test_vector),
        }
