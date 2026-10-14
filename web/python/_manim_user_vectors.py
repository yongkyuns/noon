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


def rotation_matrix_transpose_from_quaternion(quat: np.ndarray) -> list[np.ndarray]:
    quat_inv = quaternion_conjugate(quat)
    return [
        quaternion_mult(quat, [0, *basis], quat_inv)[1:]
        for basis in [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    ]


def rotation_matrix_from_quaternion(quat: np.ndarray) -> np.ndarray:
    import numpy as np
    return np.transpose(rotation_matrix_transpose_from_quaternion(quat))


def normalize_along_axis(array: np.ndarray, axis: np.ndarray) -> np.ndarray:
    """Preserve the original in-place NumPy operation on explicit user arrays."""
    import numpy as np
    norms = np.sqrt((array * array).sum(axis))
    norms[norms == 0] = 1
    buffed_norms = np.repeat(norms, array.shape[axis]).reshape(array.shape)
    array /= buffed_norms
    return array


def thick_diagonal(dim: int, thickness: int = 2) -> MatrixMN:
    import numpy as np
    rows = np.arange(dim).repeat(dim).reshape((dim, dim))
    columns = np.transpose(rows)
    return (np.abs(rows - columns) < thickness).astype("uint8")


def rotation_about_z(angle: float) -> np.ndarray:
    import numpy as np
    c, s = np.cos(angle), np.sin(angle)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def rotation_matrix(
    angle: float,
    axis: Vector3DLike,
    homogeneous: bool = False,
) -> np.ndarray:
    """Rotate explicit user vectors with the pinned SciPy conversion."""
    import numpy as np
    from scipy.spatial.transform import Rotation
    matrix = Rotation.from_rotvec(angle * normalize(axis)).as_matrix()
    if not homogeneous:
        return matrix
    result = np.eye(4)
    result[:3, :3] = matrix
    return result


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


def cross(v1: Vector3DLike, v2: Vector3DLike) -> Vector3D:
    import numpy as np
    return np.array([v1[1] * v2[2] - v1[2] * v2[1], v1[2] * v2[0] - v1[0] * v2[2], v1[0] * v2[1] - v1[1] * v2[0]])


def line_intersection(line1: Sequence[np.ndarray], line2: Sequence[np.ndarray]) -> np.ndarray:
    import numpy as np
    if any(np.array([line1, line2])[:, :, 2].reshape(-1)):
        raise ValueError('Coords must be in the xy-plane.')
    padded = (np.pad(np.array(i)[:, :2], ((0, 0), (0, 1)), constant_values=1) for i in (line1, line2))
    line1, line2 = (cross(*i) for i in padded)
    x, y, z = cross(line1, line2)
    if z == 0:
        raise ValueError('The lines are parallel, there is no unique intersection point.')
    return np.array([x / z, y / z, 0])


def find_intersection(p0s: Point3DLike_Array, v0s: Vector3DLike_Array, p1s: Point3DLike_Array, v1s: Vector3DLike_Array, threshold: float=1e-05) -> list[Point3D]:
    import numpy as np
    result = []
    for p0, v0, p1, v1 in zip(p0s, v0s, p1s, v1s, strict=True):
        normal = cross(v1, cross(v0, v1))
        denom = max(np.dot(v0, normal), threshold)
        result += [p0 + np.dot(p1 - p0, normal) / denom * v0]
    return result


def get_winding_number(points: Sequence[np.ndarray]) -> float:
    from _manim_iterables import adjacent_pairs
    total_angle: float = 0
    for p1, p2 in adjacent_pairs(points):
        d_angle = angle_of_vector(p2) - angle_of_vector(p1)
        d_angle = ((d_angle + math.pi) % math.tau) - math.pi
        total_angle += d_angle
    val: float = total_angle / math.tau
    return val


def bezier(points: Point3D_Array | Sequence[Point3D_Array]) -> Callable[[float | ColVector], Point3D_Array]:
    import numpy as np
    P = np.asarray(points)
    degree = P.shape[0] - 1
    if degree == 0:

        def zero_bezier(t: float | ColVector) -> Point3D | Point3D_Array:
            return np.ones_like(t) * P[0]
        return zero_bezier
    if degree == 1:

        def linear_bezier(t: float | ColVector) -> Point3D | Point3D_Array:
            return P[0] + t * (P[1] - P[0])
        return linear_bezier
    if degree == 2:

        def quadratic_bezier(t: float | ColVector) -> Point3D | Point3D_Array:
            t2 = t * t
            mt = 1 - t
            mt2 = mt * mt
            return mt2 * P[0] + 2 * t * mt * P[1] + t2 * P[2]
        return quadratic_bezier
    if degree == 3:

        def cubic_bezier(t: float | ColVector) -> Point3D | Point3D_Array:
            t2 = t * t
            t3 = t2 * t
            mt = 1 - t
            mt2 = mt * mt
            mt3 = mt2 * mt
            return mt3 * P[0] + 3 * t * mt2 * P[1] + 3 * t2 * mt * P[2] + t3 * P[3]
        return cubic_bezier

    def nth_grade_bezier(t: float | ColVector) -> Point3D | Point3D_Array:
        is_scalar = not isinstance(t, np.ndarray)
        if is_scalar:
            B = np.empty((1, *P.shape))
        else:
            assert isinstance(t, np.ndarray)
            t = t.reshape(-1, *[1 for dim in P.shape])
            B = np.empty((t.shape[0], *P.shape))
        B[:] = P
        for i in range(degree):
            B[:, :degree - i] += t * (B[:, 1:degree - i + 1] - B[:, :degree - i])
        if is_scalar:
            val: Point3D = B[0, 0]
            return val
        return B[:, 0]
    return nth_grade_bezier


def partial_bezier_points(points: BezierPointsLike, a: float, b: float) -> BezierPoints:
    import numpy as np
    if a == 1:
        arr = np.array(points)
        arr[:] = arr[-1]
        return arr
    if b == 0:
        arr = np.array(points)
        arr[:] = arr[0]
        return arr
    points = np.asarray(points)
    degree = points.shape[0] - 1
    if degree == 3:
        ma, mb = (1 - a, 1 - b)
        a2, b2, ma2, mb2 = (a * a, b * b, ma * ma, mb * mb)
        a3, b3, ma3, mb3 = (a2 * a, b2 * b, ma2 * ma, mb2 * mb)
        portion_matrix = np.array([[ma3, 3 * ma2 * a, 3 * ma * a2, a3], [ma2 * mb, 2 * ma * a * mb + ma2 * b, a2 * mb + 2 * ma * a * b, a2 * b], [ma * mb2, a * mb2 + 2 * ma * mb * b, 2 * a * mb * b + ma * b2, a * b2], [mb3, 3 * mb2 * b, 3 * mb * b2, b3]])
        return portion_matrix @ points
    if degree == 2:
        ma, mb = (1 - a, 1 - b)
        portion_matrix = np.array([[ma * ma, 2 * a * ma, a * a], [ma * mb, a * mb + ma * b, a * b], [mb * mb, 2 * b * mb, b * b]])
        return portion_matrix @ points
    if degree == 1:
        direction = points[1] - points[0]
        return np.array([points[0] + a * direction, points[0] + b * direction])
    if degree == 0:
        return points
    arr = np.array(points, dtype=float)
    N = arr.shape[0]
    if a != 0:
        for i in range(1, N):
            arr[:N - i] += a * (arr[1:N - i + 1] - arr[:N - i])
    if b != 1:
        mu = (1 - b) / (1 - a)
        for i in range(1, N):
            arr[i:] += mu * (arr[i - 1:-1] - arr[i:])
    return arr


def split_bezier(points: BezierPointsLike, t: float) -> Spline:
    import numpy as np
    points = np.asarray(points)
    N, dim = points.shape
    degree = N - 1
    if degree == 3:
        mt = 1 - t
        mt2 = mt * mt
        mt3 = mt2 * mt
        t2 = t * t
        t3 = t2 * t
        two_mt_t = 2 * mt * t
        three_mt2_t = 3 * mt2 * t
        three_mt_t2 = 3 * mt * t2
        split_matrix = np.array([[1, 0, 0, 0], [mt, t, 0, 0], [mt2, two_mt_t, t2, 0], [mt3, three_mt2_t, three_mt_t2, t3], [mt3, three_mt2_t, three_mt_t2, t3], [0, mt2, two_mt_t, t2], [0, 0, mt, t], [0, 0, 0, 1]])
        return split_matrix @ points
    if degree == 2:
        mt = 1 - t
        mt2 = mt * mt
        t2 = t * t
        two_tmt = 2 * t * mt
        split_matrix = np.array([[1, 0, 0], [mt, t, 0], [mt2, two_tmt, t2], [mt2, two_tmt, t2], [0, mt, t], [0, 0, 1]])
        return split_matrix @ points
    if degree == 1:
        middle = points[0] + t * (points[1] - points[0])
        return np.array([points[0], middle, middle, points[1]])
    if degree == 0:
        return np.array([points[0], points[0]])
    arr = np.empty((2, N, dim))
    arr[1] = points
    arr[0, 0] = points[0]
    for i in range(1, N):
        arr[1, :N - i] += t * (arr[1, 1:N - i + 1] - arr[1, :N - i])
        arr[0, i] = arr[1, 0]
    return arr.reshape(2 * N, dim)


def is_closed(points: Point3D_Array) -> bool:
    start, end = (points[0], points[-1])
    rtol = 1e-05
    atol = 1e-08
    tolerance = atol + rtol * start
    if abs(end[0] - start[0]) > tolerance[0]:
        return False
    if abs(end[1] - start[1]) > tolerance[1]:
        return False
    return bool(abs(end[2] - start[2]) <= tolerance[2])


def straight_path() -> PathFuncType:
    from _manim_interpolation import interpolate
    return interpolate


# Memoized coefficients operate only on explicit caller-owned numeric arrays.
_USER_SUBDIVISION_MATRICES = [{} for _ in range(4)]


def _get_subdivision_matrix(n_points: int, n_divisions: int) -> MatrixMN:
    import numpy as np
    if n_points not in (1, 2, 3, 4):
        raise NotImplementedError('This function does not support subdividing Bézier curves with 0 or more than 4 control points.')
    subdivision_matrix = _USER_SUBDIVISION_MATRICES[n_points - 1].get(n_divisions, None)
    if subdivision_matrix is not None:
        return subdivision_matrix
    subdivision_matrix = np.empty((n_points * n_divisions, n_points))
    if n_points == 4:
        for i in range(n_divisions):
            i2 = i * i
            i3 = i2 * i
            ip1 = i + 1
            ip12 = ip1 * ip1
            ip13 = ip12 * ip1
            nmi = n_divisions - i
            nmi2 = nmi * nmi
            nmi3 = nmi2 * nmi
            nmim1 = nmi - 1
            nmim12 = nmim1 * nmim1
            nmim13 = nmim12 * nmim1
            subdivision_matrix[4 * i:4 * (i + 1)] = np.array([[nmi3, 3 * nmi2 * i, 3 * nmi * i2, i3], [nmi2 * nmim1, 2 * nmi * nmim1 * i + nmi2 * ip1, nmim1 * i2 + 2 * nmi * i * ip1, i2 * ip1], [nmi * nmim12, nmim12 * i + 2 * nmi * nmim1 * ip1, 2 * nmim1 * i * ip1 + nmi * ip12, i * ip12], [nmim13, 3 * nmim12 * ip1, 3 * nmim1 * ip12, ip13]])
        subdivision_matrix /= n_divisions * n_divisions * n_divisions
    elif n_points == 3:
        for i in range(n_divisions):
            ip1 = i + 1
            nmi = n_divisions - i
            nmim1 = nmi - 1
            subdivision_matrix[3 * i:3 * (i + 1)] = np.array([[nmi * nmi, 2 * i * nmi, i * i], [nmi * nmim1, i * nmim1 + ip1 * nmi, i * ip1], [nmim1 * nmim1, 2 * ip1 * nmim1, ip1 * ip1]])
        subdivision_matrix /= n_divisions * n_divisions
    elif n_points == 2:
        aux_range = np.arange(n_divisions + 1)
        subdivision_matrix[::2, 1] = aux_range[:-1]
        subdivision_matrix[1::2, 1] = aux_range[1:]
        subdivision_matrix[:, 0] = subdivision_matrix[::-1, 1]
        subdivision_matrix /= n_divisions
    elif n_points == 1:
        subdivision_matrix[:] = 1
    _USER_SUBDIVISION_MATRICES[n_points - 1][n_divisions] = subdivision_matrix
    return subdivision_matrix


def subdivide_bezier(points: BezierPointsLike, n_divisions: int) -> Spline:
    import numpy as np
    points = np.asarray(points)
    if n_divisions == 1:
        return points
    N, dim = points.shape
    if N <= 4:
        subdivision_matrix = _get_subdivision_matrix(N, n_divisions)
        return subdivision_matrix @ points
    beziers = np.empty((n_divisions, N, dim))
    beziers[-1] = points
    for curve_num in range(n_divisions - 1, 0, -1):
        curr = beziers[curve_num]
        prev = beziers[curve_num - 1]
        prev[0] = curr[0]
        a = (n_divisions - curve_num) / (n_divisions - curve_num + 1)
        for i in range(1, N):
            curr[:N - i] += a * (curr[1:N - i + 1] - curr[:N - i])
            prev[i] = curr[0]
    return beziers.reshape(n_divisions * N, dim)


def bezier_remap(bezier_tuples: BezierPointsLike_Array, new_number_of_curves: int) -> BezierPoints_Array:
    import numpy as np
    bezier_tuples = np.asarray(bezier_tuples)
    current_number_of_curves, nppc, dim = bezier_tuples.shape
    repeat_indices = np.arange(new_number_of_curves, dtype='i') * current_number_of_curves // new_number_of_curves
    split_factors = np.zeros(current_number_of_curves, dtype='i')
    np.add.at(split_factors, repeat_indices, 1)
    new_tuples = np.empty((new_number_of_curves, nppc, dim))
    index = 0
    for curve, sf in zip(bezier_tuples, split_factors, strict=True):
        new_tuples[index:index + sf] = subdivide_bezier(curve, sf).reshape(sf, nppc, dim)
        index += sf
    return new_tuples
