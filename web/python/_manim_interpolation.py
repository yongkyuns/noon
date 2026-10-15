"""Interpolation of explicit Python user data, independent of engine playback."""
from __future__ import annotations


def interpolate(start: float | Point3D, end: float | Point3D,
                alpha: float | ColVector) -> float | ColVector | Point3D | Point3D_Array:
    return (1 - alpha) * start + alpha * end


def integer_interpolate(start: float, end: float, alpha: float) -> tuple[int, float]:
    if alpha >= 1:
        return (int(end - 1), 1.0)
    if alpha <= 0:
        return (int(start), 0)
    value = int(interpolate(start, end, alpha))
    return (value, ((end - start) * alpha) % 1)


def mid(start: float | Point3D, end: float | Point3D) -> float | Point3D:
    return (start + end) / 2.0


def inverse_interpolate(start: float | Point3D, end: float | Point3D,
                        value: float | Point3D) -> float | Point3D:
    import numpy as np
    return np.true_divide(value - start, end - start)


def match_interpolate(new_start: float, new_end: float, old_start: float,
                      old_end: float, old_value: float | Point3D) -> float | Point3D:
    return interpolate(new_start, new_end, inverse_interpolate(old_start, old_end, old_value))
