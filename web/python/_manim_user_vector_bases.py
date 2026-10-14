"""Vector algebra on explicit user values; canonical scene state remains in Rust."""

from __future__ import annotations

from functools import cache
from _manim_user_vectors import cross, normalize


@cache
def _down_direction():
    import numpy as np
    return np.array([0., -1., 0.])


def norm_squared(v: float) -> float:
    import numpy as np
    val: float = np.dot(v, v)
    return val


def get_unit_normal(v1: Vector3DLike, v2: Vector3DLike, tol: float = 1e-6) -> Vector3D:
    """Original user-vector normal, including aligned and null-vector rules."""
    import numpy as np
    np_v1 = np.asarray(v1)
    np_v2 = np.asarray(v2)
    div1, div2 = max(np.abs(np_v1)), max(np.abs(np_v2))
    if div1 == 0.0:
        if div2 == 0.0:
            return _down_direction()
        u = np_v2 / div2
    elif div2 == 0.0:
        u = np_v1 / div1
    else:
        u1, u2 = np_v1 / div1, np_v2 / div2
        cp = cross(u1, u2)
        cp_norm = np.sqrt(norm_squared(cp))
        if cp_norm > tol:
            return cp / cp_norm
        u = u1
    if abs(u[0]) < tol and abs(u[1]) < tol:
        return _down_direction()
    cp = np.array([-u[0] * u[2], -u[1] * u[2], u[0] * u[0] + u[1] * u[1]])
    cp_norm = np.sqrt(norm_squared(cp))
    return cp / cp_norm


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
