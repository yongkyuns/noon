"""Small, slow M0 test oracle; never imported by the Noon product.

The finite encoded-LDR operator is specified in the adjacent README. This
module has no scene store, scheduler, GPU path, or production Noon imports.
"""
from __future__ import annotations

import math
from collections.abc import Sequence


def number(value: float, low: float = 0.0, high: float = math.inf) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError("expected a finite number")
    try:
        value = float(value)
    except OverflowError as error:
        raise ValueError("number outside its finite range") from error
    if not math.isfinite(value) or not low <= value <= high:
        raise ValueError("number outside its finite range")
    return value


def radius_pixels(radius: float, unit: str, height: int, view_height: float) -> float:
    radius = number(radius)
    number(height, 1)
    number(view_height, math.ulp(0.0))
    if not isinstance(height, int) or isinstance(height, bool):
        raise ValueError("output height must be an integer")
    if unit == "pixels":
        return radius
    if unit == "scene":
        return number(radius * height / view_height)
    raise ValueError("unsupported unit")


def checked_grid(grid: Sequence[Sequence[float]]) -> tuple[int, int]:
    if not grid or not grid[0]:
        raise ValueError("empty field")
    width, height = len(grid[0]), len(grid)
    # A deliberate test-oracle bound, NOT a product capability limit.
    if width > 65 or height > 65 or any(len(row) != width for row in grid):
        raise ValueError("ragged or oversized reference field")
    for row in grid:
        for value in row:
            number(value)
    return width, height


def convolve(field: Sequence[Sequence[float]], sigma: float) -> list[list[float]]:
    """Direct 2D sum, unlike a production separable/multiresolution blur.

    Inputs include their output padding. Beyond that canvas is transparent;
    edge weights are never renormalized. No intermediate UNORM quantization.
    """
    width, height = checked_grid(field)
    sigma = number(sigma, 0.0, 16.0)
    if sigma == 0:
        return [list(row) for row in field]
    support = math.ceil(3 * sigma)
    weights = [
        (dx, dy, math.exp(-0.5 * (math.hypot(dx, dy) / sigma) * (math.hypot(dx, dy) / sigma)))
        for dy in range(-support, support + 1)
        for dx in range(-support, support + 1)
    ]
    total = math.fsum(weight for _, _, weight in weights)
    return [
        [
            math.fsum(
                field[y - dy][x - dx] * weight
                for dx, dy, weight in weights
                if 0 <= x - dx < width and 0 <= y - dy < height
            ) / total
            for x in range(width)
        ]
        for y in range(height)
    ]


def rgba(pixel: Sequence[float]) -> tuple[float, ...]:
    if len(pixel) != 4:
        raise ValueError("expected RGBA")
    result = tuple(number(value, 0, 1) for value in pixel)
    if any(value > result[3] for value in result[:3]):
        raise ValueError("expected premultiplied RGBA")
    return result


def over(front: Sequence[float], back: Sequence[float]) -> tuple[float, ...]:
    front, back = rgba(front), rgba(back)
    return tuple(front[i] + (1 - front[3]) * back[i] for i in range(4))


def glow_pixel(source: Sequence[float], blurred_mask: float,
               tint: Sequence[float], intensity: float, opacity: float = 1.0
               ) -> tuple[float, ...]:
    """Compose the tinted halo BEHIND the original encoded-premultiplied source.

    Source excludes final scope opacity. Tint is straight encoded RGBA, not
    premultiplied. Radius-zero bypass belongs to the caller by specification.
    """
    source = rgba(source)
    if len(tint) != 4:
        raise ValueError("expected tint RGBA")
    tint = tuple(number(value, 0, 1) for value in tint)
    intensity, opacity = number(intensity, 0, 8), number(opacity, 0, 1)
    alpha = min(1.0, intensity * tint[3] * number(blurred_mask, 0, 1))
    halo = tuple(channel * alpha for channel in tint[:3]) + (alpha,)
    return tuple(value * opacity for value in over(source, halo))


def smooth(progress: float) -> float:
    p = number(progress, 0, 1)
    if p in (0, 1):
        return p
    # Algebraically equivalent to the normalized logistic definition, expressed
    # through tanh. No production lowering/rate-function helper is imported.
    return (math.tanh(5 * (p - 0.5)) / math.tanh(2.5) + 1) / 2


def sample(start: float, target: float, progress: float, *, pulse: bool = False) -> float:
    start, target, p = number(start, 0, 8), number(target, 0, 8), number(progress, 0, 1)
    if pulse:
        if p in (0, 1):
            return start
        p = smooth(1 - abs(2 * p - 1))
    return start + (target - start) * p


def restore(captured: dict[str, float], current: dict[str, float],
            still_owned: set[str]) -> dict[str, float]:
    """A value-table oracle, not an implementation of runtime ownership leases."""
    if not still_owned <= captured.keys() or not captured.keys() <= current.keys():
        raise ValueError("invalid ownership witness")
    return {key: captured[key] if key in still_owned else value
            for key, value in current.items()}


def assert_field(actual: Sequence[Sequence[float]], expected: Sequence[Sequence[float]],
                 absolute: float = 1e-12) -> None:
    """Fail closed on nonfinite values, shape errors and every out-of-budget pixel."""
    absolute = number(absolute)
    if checked_grid(actual) != checked_grid(expected):
        raise AssertionError("field dimensions differ")
    for y, (observed, reference) in enumerate(zip(actual, expected)):
        for x, (a, b) in enumerate(zip(observed, reference)):
            if abs(a - b) > absolute:
                raise AssertionError(f"pixel ({x}, {y}): {a} != {b}")
