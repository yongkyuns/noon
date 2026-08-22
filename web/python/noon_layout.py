"""Small semantic layout helpers for Noon examples and authoring code.

The helpers intentionally stay renderer-independent: they only produce tuple-like
2D coordinates that the core Noon authoring API already accepts.
"""

from __future__ import annotations

import math


def _finite(name: str, value: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


class Vec2(tuple):
    """Tuple-compatible 2D vector with readable arithmetic for scene layout."""

    __slots__ = ()

    def __new__(cls, x: float = 0.0, y: float = 0.0) -> Vec2:
        return tuple.__new__(cls, (_finite("x", x), _finite("y", y)))

    @property
    def x(self) -> float:
        return self[0]

    @property
    def y(self) -> float:
        return self[1]

    def __add__(self, other: object) -> Vec2:
        rhs = as_vec2(other)
        return Vec2(self.x + rhs.x, self.y + rhs.y)

    def __sub__(self, other: object) -> Vec2:
        rhs = as_vec2(other)
        return Vec2(self.x - rhs.x, self.y - rhs.y)

    def __neg__(self) -> Vec2:
        return Vec2(-self.x, -self.y)

    def __mul__(self, scalar: float) -> Vec2:
        factor = _finite("scalar", scalar)
        return Vec2(self.x * factor, self.y * factor)

    def __rmul__(self, scalar: float) -> Vec2:
        return self * scalar

    def __truediv__(self, scalar: float) -> Vec2:
        divisor = _finite("scalar", scalar)
        if divisor == 0.0:
            raise ZeroDivisionError("cannot divide Vec2 by zero")
        return Vec2(self.x / divisor, self.y / divisor)

    def length(self) -> float:
        return math.hypot(self.x, self.y)

    def normalized(self) -> Vec2:
        magnitude = self.length()
        if magnitude == 0.0:
            raise ValueError("direction must be non-zero")
        return self / magnitude

    def rotated(self, angle: float) -> Vec2:
        theta = _finite("angle", angle)
        cosine = math.cos(theta)
        sine = math.sin(theta)
        return Vec2(
            self.x * cosine - self.y * sine,
            self.x * sine + self.y * cosine,
        )


def as_vec2(value: object) -> Vec2:
    if isinstance(value, Vec2):
        return value
    if isinstance(value, (tuple, list)) and len(value) == 2:
        return Vec2(value[0], value[1])
    raise TypeError("expected a Vec2 or a two-value tuple/list")


ORIGIN = Vec2(0.0, 0.0)
LEFT = Vec2(-1.0, 0.0)
RIGHT = Vec2(1.0, 0.0)
UP = Vec2(0.0, 1.0)
DOWN = Vec2(0.0, -1.0)


def arrange(
    count: int,
    *,
    direction: Vec2 | tuple[float, float] = RIGHT,
    spacing: float = 1.0,
    center: Vec2 | tuple[float, float] = ORIGIN,
) -> tuple[Vec2, ...]:
    """Return evenly spaced positions centered along a direction vector."""

    if isinstance(count, bool) or not isinstance(count, int):
        raise TypeError("count must be an integer")
    if count <= 0:
        raise ValueError("count must be positive")
    gap = _finite("spacing", spacing)
    if gap < 0.0:
        raise ValueError("spacing must be non-negative")

    axis = as_vec2(direction).normalized()
    midpoint = as_vec2(center)
    first_offset = -0.5 * (count - 1) * gap
    return tuple(midpoint + axis * (first_offset + index * gap) for index in range(count))


def grid(
    rows: int,
    columns: int,
    *,
    spacing: Vec2 | tuple[float, float] = Vec2(1.0, 1.0),
    center: Vec2 | tuple[float, float] = ORIGIN,
) -> tuple[Vec2, ...]:
    """Return row-major grid positions, ordered left-to-right then top-to-bottom."""

    for name, value in (("rows", rows), ("columns", columns)):
        if isinstance(value, bool) or not isinstance(value, int):
            raise TypeError(f"{name} must be an integer")
        if value <= 0:
            raise ValueError(f"{name} must be positive")

    gaps = as_vec2(spacing)
    if gaps.x < 0.0 or gaps.y < 0.0:
        raise ValueError("grid spacing must be non-negative")
    midpoint = as_vec2(center)
    x_positions = arrange(columns, direction=RIGHT, spacing=gaps.x)
    y_positions = arrange(rows, direction=DOWN, spacing=gaps.y)
    return tuple(midpoint + Vec2(x.x, y.y) for y in y_positions for x in x_positions)


def polar(
    count: int,
    *,
    radius: float,
    center: Vec2 | tuple[float, float] = ORIGIN,
    start_angle: float = math.pi / 2.0,
) -> tuple[Vec2, ...]:
    """Return evenly spaced positions around a circle."""

    if isinstance(count, bool) or not isinstance(count, int):
        raise TypeError("count must be an integer")
    if count <= 0:
        raise ValueError("count must be positive")
    r = _finite("radius", radius)
    if r < 0.0:
        raise ValueError("radius must be non-negative")
    origin = as_vec2(center)
    start = _finite("start_angle", start_angle)
    step = math.tau / count
    return tuple(
        origin + Vec2(math.cos(start + index * step), math.sin(start + index * step)) * r
        for index in range(count)
    )
