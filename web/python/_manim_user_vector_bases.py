"""Vector algebra on explicit user values; canonical scene state remains in Rust."""

from __future__ import annotations

from _manim_user_vectors import cross, normalize


def norm_squared(v: float) -> float:
    import numpy as np
    val: float = np.dot(v, v)
    return val


def z_to_vector(vector: np.ndarray) -> np.ndarray:
    """
    Returns some matrix in SO(3) which takes the z-axis to the
    (normalized) vector provided as an argument
    """
    import numpy as np
    RIGHT = np.array([1., 0., 0.])
    UP = np.array([0., 1., 0.])
    axis_z = normalize(vector)
    axis_y = normalize(cross(axis_z, RIGHT))
    axis_x = cross(axis_y, axis_z)
    if np.linalg.norm(axis_y) == 0:
        # the vector passed just so happened to be in the x direction.
        axis_x = normalize(cross(UP, axis_z))
        axis_y = -cross(axis_x, axis_z)

    return np.array([axis_x, axis_y, axis_z]).T


