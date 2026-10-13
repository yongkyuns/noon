"""Vector operations on explicit Python user data, independent of engine state."""
from __future__ import annotations

from collections.abc import Callable, Sequence
import math


def complex_to_R3(complex_num: complex) -> np.ndarray:
    import numpy as np
    return np.array((complex_num.real, complex_num.imag, 0))


def R3_to_complex(point: Sequence[float]) -> np.ndarray:
    return complex(*point[:2])


def complex_func_to_R3_func(
    complex_func: Callable[[complex], complex],
) -> Callable[[Point3DLike], Point3D]:
    return lambda p: complex_to_R3(complex_func(R3_to_complex(p)))


def center_of_mass(points: PointNDLike_Array) -> PointND:
    import numpy as np
    return np.average(points, 0, np.ones(len(points)))


def midpoint(point1: Sequence[float], point2: Sequence[float]) -> float | np.ndarray:
    return center_of_mass([point1, point2])


def normalize(vect: np.ndarray | tuple[float], fall_back: np.ndarray | None = None) -> np.ndarray:
    import numpy as np
    norm = np.linalg.norm(vect)
    if norm > 0:
        return np.array(vect) / norm
    return fall_back or np.zeros(len(vect))


def angle_of_vector(vector: Sequence[float] | np.ndarray) -> float:
    import numpy as np
    if isinstance(vector, np.ndarray) and len(vector.shape) > 1:
        if vector.shape[0] < 2:
            raise ValueError("Vector must have the correct dimensions. (2, n)")
        c_vec = np.empty(vector.shape[1], dtype=np.complex128)
        c_vec.real = vector[0]
        c_vec.imag = vector[1]
        return np.angle(c_vec)
    return np.angle(complex(*vector[:2]))


def angle_between_vectors(v1: np.ndarray, v2: np.ndarray) -> float:
    import numpy as np
    return 2 * np.arctan2(
        np.linalg.norm(normalize(v1) - normalize(v2)),
        np.linalg.norm(normalize(v1) + normalize(v2)),
    )


def shoelace(x_y: Point2D_Array) -> float:
    import numpy as np
    x = x_y[:, 0]
    y = x_y[:, 1]
    return np.trapezoid(y, x)


def shoelace_direction(x_y: Point2D_Array) -> str:
    return "CW" if shoelace(x_y) > 0 else "CCW"


def cross2d(a: Vector2D | Vector2D_Array, b: Vector2D | Vector2D_Array) -> ManimFloat | npt.NDArray[ManimFloat]:
    if len(a.shape) == 2:
        return a[:, 0] * b[:, 1] - a[:, 1] * b[:, 0]
    return a[0] * b[1] - b[0] * a[1]


def quaternion_mult(*quats: Sequence[float]) -> np.ndarray | list[float | np.ndarray]:
    if len(quats) == 0:
        return [1, 0, 0, 0]
    result = quats[0]
    for next_quat in quats[1:]:
        w1, x1, y1, z1 = result
        w2, x2, y2, z2 = next_quat
        result = [
            w1*w2 - x1*x2 - y1*y2 - z1*z2,
            w1*x2 + x1*w2 + y1*z2 - z1*y2,
            w1*y2 + y1*w2 + z1*x2 - x1*z2,
            w1*z2 + z1*w2 + x1*y2 - y1*x2,
        ]
    return result


def quaternion_from_angle_axis(angle: float, axis: np.ndarray, axis_normalized: bool = False) -> list[float]:
    import numpy as np
    if not axis_normalized:
        axis = normalize(axis)
    return [np.cos(angle / 2), *(np.sin(angle / 2) * axis)]


def angle_axis_from_quaternion(quaternion: Sequence[float]) -> Sequence[float]:
    import numpy as np
    axis = normalize(quaternion[1:], fall_back=np.array([1, 0, 0]))
    angle = 2 * np.arccos(quaternion[0])
    if angle > math.tau / 2:
        angle = math.tau - angle
    return angle, axis


def quaternion_conjugate(quaternion: Sequence[float]) -> np.ndarray:
    import numpy as np
    result = np.array(quaternion)
    result[1:] *= -1
    return result


def thick_diagonal(dim: int, thickness: int = 2) -> MatrixMN:
    import numpy as np
    rows = np.arange(dim).repeat(dim).reshape((dim, dim))
    columns = np.transpose(rows)
    return (np.abs(rows - columns) < thickness).astype("uint8")


def rotation_about_z(angle: float) -> np.ndarray:
    import numpy as np
    c, s = np.cos(angle), np.sin(angle)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def cartesian_to_spherical(vec: Vector3DLike) -> np.ndarray:
    import numpy as np
    norm = np.linalg.norm(vec)
    if norm == 0:
        return np.zeros(3)
    r = norm
    phi = np.arccos(vec[2] / r)
    theta = np.arctan2(vec[1], vec[0])
    return np.array([r, theta, phi])


def spherical_to_cartesian(spherical: Sequence[float]) -> np.ndarray:
    import numpy as np
    r, theta, phi = spherical
    return np.array([r*np.cos(theta)*np.sin(phi), r*np.sin(theta)*np.sin(phi), r*np.cos(phi)])
