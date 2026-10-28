"""Small, slow M0 test oracle; never imported by the Noon product.

The finite encoded-LDR operator is specified in the adjacent README. This
module has no scene store, scheduler, GPU path, or production Noon imports.
"""
from __future__ import annotations

import math
from collections.abc import Sequence
from fractions import Fraction


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


def linear_gradient_pixel(start: Sequence[float], end: Sequence[float],
                          stops: Sequence[tuple[float, Sequence[float]]],
                          point: Sequence[float], *, coverage: float = 1.0,
                          fill_opacity: float = 1.0, object_opacity: float = 1.0
                          ) -> tuple[float, ...]:
    """Independent exact-rational local-paint oracle; no production imports.

    Input decimal spellings define exact fractions. Project before pad extension,
    choose the rightmost coincident stop, interpolate encoded-premultiplied RGBA,
    then multiply coverage and the two distinct opacity domains exactly once.
    This is a point/coverage oracle, not an independent geometry rasterizer.
    """
    def fraction(value, low=-math.inf, high=math.inf):
        number(value, low, high)
        return Fraction(str(value))

    if len(start) != 2 or len(end) != 2 or len(point) != 2:
        raise ValueError("expected two-dimensional normalized local coordinates")
    a = tuple(fraction(v, 0, 1) for v in start)
    b = tuple(fraction(v, 0, 1) for v in end)
    p = tuple(fraction(v) for v in point)
    axis = tuple(b[i] - a[i] for i in range(2))
    length_squared = sum(v * v for v in axis)
    if not length_squared:
        raise ValueError("degenerate gradient axis")
    if len(stops) < 2:
        raise ValueError("expected at least two stops")
    checked = []
    for position, color in stops:
        position = fraction(position, 0, 1)
        if len(color) != 4:
            raise ValueError("expected straight encoded RGBA")
        color = tuple(fraction(v, 0, 1) for v in color)
        if checked and position < checked[-1][0]:
            raise ValueError("unordered stops")
        checked.append((position, tuple(v * color[3] for v in color[:3]) + color[3:]))
    t = sum((p[i] - a[i]) * axis[i] for i in range(2)) / length_squared
    left = None
    right = None
    for entry in checked:
        if entry[0] <= t:
            left = entry
        elif right is None:
            right = entry
    if left is None:
        color = checked[0][1]
    elif right is None:
        color = left[1]
    else:
        weight = (t - left[0]) / (right[0] - left[0])
        color = tuple((1 - weight) * x + weight * y
                      for x, y in zip(left[1], right[1]))
    opacity = (fraction(coverage, 0, 1) * fraction(fill_opacity, 0, 1)
               * fraction(object_opacity, 0, 1))
    return tuple(float(v * opacity) for v in color)


def paint_transition_pixel(from_axis, to_axis, from_stops, to_stops, alpha, point, opacities):
    """Independent rational temporal promotion followed by the spatial oracle.

    The f64/f32 engine and WGSL are not imported. Time is a caller-supplied
    normalized sample; this function owns neither scene state nor scheduling.
    """
    number(alpha, 0, 1)
    t = Fraction(str(alpha))
    def mix(a, b):
        return Fraction(str(a)) * (1 - t) + Fraction(str(b)) * t
    axis = [float(mix(a, b)) for a, b in zip(from_axis, to_axis)]
    if len(from_stops) != len(to_stops):
        raise ValueError("different stop counts")
    stops = []
    for a, b in zip(from_stops, to_stops):
        premultiplied = [mix(Fraction(str(a[i])) * Fraction(str(a[4])),
                            Fraction(str(b[i])) * Fraction(str(b[4]))) for i in range(1, 4)]
        opacity = mix(a[4], b[4])
        rgb = [float(v / opacity) if opacity else 0.0 for v in premultiplied]
        stops.append((float(mix(a[0], b[0])), (*rgb, float(opacity))))
    return linear_gradient_pixel(axis[:2], axis[2:], stops, point,
                                 fill_opacity=float(mix(*opacities)))
