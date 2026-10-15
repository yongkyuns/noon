"""Python argument coercion only; shared Rust resolves render option meanings.

Native CPython and Pyodide use this same module. This is an option API, not a
browser encoder, frame iterator, Python quality table or configuration singleton.
"""
from __future__ import annotations
from dataclasses import dataclass
from numbers import Integral, Real


@dataclass(frozen=True)
class RenderOptions:
    width: int
    height: int
    fps: tuple[int, int]
    format: str


def _rate_text(value):
    if value is None or isinstance(value, str):
        return value
    if isinstance(value, bool):
        raise TypeError("frame_rate must be numeric, not bool")
    if isinstance(value, (tuple, list)):
        if len(value) != 2 or any(isinstance(v, bool) or not isinstance(v, Integral) for v in value):
            raise TypeError("rational frame_rate must contain two integers")
        return f"{value[0]}/{value[1]}"
    if isinstance(value, Real):
        # str preserves the user's decimal spelling of an ordinary float. Rust
        # validates and parses it exactly; do not infer an NTSC rate in Python.
        if isinstance(value, Integral):
            return str(int(value))
        if hasattr(value, "numerator") and hasattr(value, "denominator"):
            return f"{value.numerator}/{value.denominator}"
        return str(value)
    raise TypeError("frame_rate must be numeric, a string or an integer ratio")


def resolve_render_options(*, quality=None, resolution=None, frame_rate=None,
                           format=None, pixel_width=None, pixel_height=None):
    from _noon_host import noonResolveRenderOptions
    if quality is not None and not isinstance(quality, str):
        raise TypeError("quality must be a string")
    if format is not None and not isinstance(format, str):
        raise TypeError("format must be a string")
    if isinstance(resolution, (tuple, list)):
        if len(resolution) != 2 or any(isinstance(v, bool) or not isinstance(v, Integral) for v in resolution):
            raise TypeError("resolution must contain two integer dimensions")
        resolution = f"{resolution[0]},{resolution[1]}"
    if resolution is not None and not isinstance(resolution, str):
        raise TypeError("resolution must be W,H text or two integer dimensions")
    dimensions = []
    for value in (pixel_width, pixel_height):
        if value is not None and (isinstance(value, bool) or not isinstance(value, Integral)):
            raise TypeError("pixel dimensions must be integers")
        # Validate the binding representation equally for PyO3 and wasm-bindgen;
        # a JS u32 conversion must not silently wrap an out-of-range Python int.
        if value is not None and not 0 <= value <= 0xffffffff:
            raise ValueError("pixel dimension is outside the u32 binding range")
        dimensions.append(None if value is None else int(value))
    from _noon_errors import engine_call
    result = engine_call(noonResolveRenderOptions, quality, resolution, _rate_text(frame_rate),
                         format, *dimensions, operation="render options")
    return RenderOptions(int(result.pixelWidth), int(result.pixelHeight),
                         (int(result.frameRateNumerator), int(result.frameRateDenominator)),
                         str(result.format))
