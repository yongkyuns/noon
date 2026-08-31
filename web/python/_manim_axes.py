"""Thin ManimCE v0.21 Axes facade over shared Rust/WASM authoring state.

Python owns only host-type coercion and unavoidable user-function evaluation. Axis
placement, tick geometry, coordinate conversion, sampling, discontinuities, smoothing,
and final plot geometry remain in the shared Rust plans.
"""

from __future__ import annotations

import json
import math
from collections.abc import Iterable
from typing import Any, Callable

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared

try:
    from js import noonCreateAxesAuthoringPlan as _create_axes_plan
except ImportError:  # Native CPython has no WASM authoring plan.
    _create_axes_plan = None


_DEFAULT_X_RANGE = (-7.0, 7.0, 1.0)
_DEFAULT_Y_RANGE = (-4.0, 4.0, 1.0)
_DEFAULT_X_LENGTH = 12.0
_DEFAULT_Y_LENGTH = 6.0
_DEFAULT_PLOT_COLOR = _base.color_from_hex("#FFFF00")
_DEFAULT_DISCONTINUITY_DT = 1.0e-8

_SUPPORTED_AXIS_OPTIONS = {
    "include_ticks",
    "tick_size",
    "numbers_with_elongated_ticks",
    "longer_tick_multiple",
    "stroke_width",
    "color",
    "include_tip",
    "include_numbers",
    "numbers_to_include",
}


def _finite(name: str, value: object) -> float:
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


def _range3(name: str, value: object | None, default: tuple[float, float, float]) -> list[float]:
    if value is None:
        return list(default)
    try:
        values = list(value)  # type: ignore[arg-type]
    except TypeError as error:
        raise TypeError(f"{name} must contain 2 or 3 numeric values") from error
    if len(values) == 2:
        values.append(1.0)
    if len(values) != 3:
        raise ValueError(f"{name} must contain 2 or 3 values")
    return [_finite(f"{name}[{index}]", item) for index, item in enumerate(values)]


def _float_list(name: str, value: object) -> list[float]:
    try:
        values = list(value)  # type: ignore[arg-type]
    except TypeError as error:
        raise TypeError(f"{name} must be an iterable of numbers") from error
    return [_finite(f"{name}[{index}]", item) for index, item in enumerate(values)]


def _color_payload(value: object) -> list[float]:
    color = _shared._phase_b._as_color("color", value)
    return [float(color.red), float(color.green), float(color.blue), float(color.alpha)]


def _axis_config(name: str, value: dict[str, object] | None) -> dict[str, object]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise TypeError(f"{name} must be a dict or None")
    unknown = sorted(set(value) - _SUPPORTED_AXIS_OPTIONS)
    if unknown:
        raise NotImplementedError(
            f"unsupported retained Axes {name} option(s): {', '.join(unknown)}"
        )

    result: dict[str, object] = {}
    for key, item in value.items():
        if key == "color":
            result[key] = _color_payload(item)
        elif key in {"numbers_with_elongated_ticks", "numbers_to_include"}:
            result[key] = _float_list(f"{name}.{key}", item)
        elif key in {"tick_size", "stroke_width"}:
            result[key] = _finite(f"{name}.{key}", item)
        elif key == "longer_tick_multiple":
            if isinstance(item, bool):
                raise TypeError(f"{name}.{key} must be an integer")
            result[key] = int(item)
        elif key in {"include_ticks", "include_tip", "include_numbers"}:
            if not isinstance(item, bool):
                raise TypeError(f"{name}.{key} must be a bool")
            result[key] = item
    return result


def _snapshot_line(snapshot: object) -> _compat.Line:
    if _shared._create_handle is None:
        raise RuntimeError("shared Mobject handles are unavailable")
    line = object.__new__(_compat.Line)
    snapshot_json = json.dumps(snapshot, separators=(",", ":"), allow_nan=False)
    _shared._attach_shared_handle(line, _shared._create_handle(snapshot_json))
    return line


def _snapshot_curve(snapshot_json: str) -> ParametricFunction:
    if _shared._create_handle is None:
        raise RuntimeError("shared Mobject handles are unavailable")
    curve = object.__new__(ParametricFunction)
    _shared._attach_shared_handle(curve, _shared._create_handle(snapshot_json))
    return curve


def _blocked_axes_transform(name: str):
    def blocked(self: Axes, *args: object, **kwargs: object):
        del self, args, kwargs
        raise NotImplementedError(
            f"Axes.{name} requires shared affine coordinate-state synchronization"
        )

    blocked.__name__ = name
    return blocked


class ParametricFunction(_compat.VMobject):
    """Retained curve produced by shared plot authoring.

    Direct host-callback construction remains separate; the initial source-compatible
    surface is created through :meth:`Axes.plot` so the curve shares its Axes state.
    """

    def __init__(self, *args: object, **kwargs: object) -> None:
        del args, kwargs
        raise NotImplementedError(
            "direct ParametricFunction construction is not implemented in the shared browser subset"
        )


class FunctionGraph(ParametricFunction):
    """Source-visible FunctionGraph type pending its direct shared callback plan."""


class Axes(_compat.VGroup):
    """Initial retained linear Axes subset backed by one shared Rust/WASM plan."""

    def __init__(
        self,
        x_range: object | None = None,
        y_range: object | None = None,
        x_length: float | None = _DEFAULT_X_LENGTH,
        y_length: float | None = _DEFAULT_Y_LENGTH,
        axis_config: dict[str, object] | None = None,
        x_axis_config: dict[str, object] | None = None,
        y_axis_config: dict[str, object] | None = None,
        tips: bool = True,
        **kwargs: Any,
    ) -> None:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported retained Axes option(s): {unsupported}")
        if _create_axes_plan is None:
            raise RuntimeError("shared Axes authoring is available only in the browser WASM frontend")
        if x_length is None or y_length is None:
            raise NotImplementedError("automatic Axes lengths are not implemented")
        if not isinstance(tips, bool):
            raise TypeError("tips must be a bool")

        self.x_range = _range3("x_range", x_range, _DEFAULT_X_RANGE)
        self.y_range = _range3("y_range", y_range, _DEFAULT_Y_RANGE)
        self.x_length = _finite("x_length", x_length)
        self.y_length = _finite("y_length", y_length)
        request = {
            "x_range": self.x_range,
            "y_range": self.y_range,
            "x_length": self.x_length,
            "y_length": self.y_length,
            "tips": tips,
            "axis_config": _axis_config("axis_config", axis_config),
            "x_axis_config": _axis_config("x_axis_config", x_axis_config),
            "y_axis_config": _axis_config("y_axis_config", y_axis_config),
        }
        self._axes_plan = _create_axes_plan(
            json.dumps(request, separators=(",", ":"), allow_nan=False)
        )

        snapshots = json.loads(str(self._axes_plan.childrenJson()))
        split = int(self._axes_plan.xChildCount)
        if split <= 0 or split >= len(snapshots):
            raise RuntimeError("shared Axes plan returned an invalid family boundary")
        x_members = [_snapshot_line(snapshot) for snapshot in snapshots[:split]]
        y_members = [_snapshot_line(snapshot) for snapshot in snapshots[split:]]
        self.x_axis = _compat.VGroup(*x_members)
        self.y_axis = _compat.VGroup(*y_members)
        self.axes = _compat.VGroup(self.x_axis, self.y_axis)
        super().__init__(self.x_axis, self.y_axis)

    def coords_to_point(self, *coords: object) -> _base.Vec2:
        if len(coords) not in (2, 3):
            raise TypeError("Axes.coords_to_point expects x, y, and optional zero z")
        x = _finite("x", coords[0])
        y = _finite("y", coords[1])
        if len(coords) == 3 and not math.isclose(_finite("z", coords[2]), 0.0, abs_tol=1.0e-12):
            raise NotImplementedError("retained Axes currently supports only z=0")
        point = json.loads(str(self._axes_plan.coordsToPointJson(x, y)))
        return _base.Vec2(float(point[0]), float(point[1]))

    c2p = coords_to_point

    def point_to_coords(self, point: object) -> tuple[float, float]:
        value = _compat._as_vec2(point)
        coords = json.loads(str(self._axes_plan.pointToCoordsJson(value.x, value.y)))
        return float(coords[0]), float(coords[1])

    p2c = point_to_coords

    def get_origin(self) -> _base.Vec2:
        return self.coords_to_point(0.0, 0.0)

    def get_axes(self) -> _compat.VGroup:
        return self.axes

    def get_all_ranges(self) -> list[list[float]]:
        return [list(self.x_range), list(self.y_range)]

    def plot(
        self,
        function: Callable[[float], object],
        x_range: object | None = None,
        *,
        color: object = _DEFAULT_PLOT_COLOR,
        use_smoothing: bool = True,
        discontinuities: Iterable[float] | None = None,
        dt: float = _DEFAULT_DISCONTINUITY_DT,
        **kwargs: Any,
    ) -> ParametricFunction:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported retained Axes.plot option(s): {unsupported}")
        if not callable(function):
            raise TypeError("Axes.plot function must be callable")
        if not isinstance(use_smoothing, bool):
            raise TypeError("use_smoothing must be a bool")

        plot_range = None if x_range is None else _range3("x_range", x_range, tuple(self.x_range))
        if x_range is not None:
            try:
                original_length = len(x_range)  # type: ignore[arg-type]
            except TypeError:
                original_length = 3
            if original_length == 2:
                plot_range = plot_range[:2]
        discontinuity_values = (
            None
            if discontinuities is None
            else _float_list("discontinuities", discontinuities)
        )
        plot_request = {
            "plot_range": plot_range,
            "discontinuities": discontinuity_values,
            "discontinuity_dt": _finite("dt", dt),
            "use_smoothing": use_smoothing,
        }
        request_json = json.dumps(plot_request, separators=(",", ":"), allow_nan=False)
        parameter_subpaths = json.loads(str(self._axes_plan.plotParametersJson(request_json)))
        value_subpaths: list[list[float]] = []
        for parameters in parameter_subpaths:
            values: list[float] = []
            for parameter in parameters:
                parameter_value = float(parameter)
                result = float(function(parameter_value))
                if not math.isfinite(result):
                    raise ValueError(
                        f"Axes.plot function returned a non-finite value at {parameter_value}: {result}"
                    )
                values.append(result)
            value_subpaths.append(values)

        snapshot_json = str(
            self._axes_plan.finishPlotSnapshotJson(
                request_json,
                json.dumps(value_subpaths, separators=(",", ":"), allow_nan=False),
            )
        )
        graph = _snapshot_curve(snapshot_json)
        graph.function = function
        graph.underlying_function = function
        graph.x_range = list(self.x_range if x_range is None else plot_range)
        graph.set_color(color)
        return graph

    # A Python-only family transform would move visible lines while leaving c2p/p2c
    # and subsequently-created plots on stale shared coordinates. Fail explicitly
    # until the shared plan owns affine Axes placement too.
    shift = _blocked_axes_transform("shift")
    move_to = _blocked_axes_transform("move_to")
    center = _blocked_axes_transform("center")
    set_x = _blocked_axes_transform("set_x")
    set_y = _blocked_axes_transform("set_y")
    scale = _blocked_axes_transform("scale")
    rotate = _blocked_axes_transform("rotate")
    next_to = _blocked_axes_transform("next_to")
    align_to = _blocked_axes_transform("align_to")
    copy = _blocked_axes_transform("copy")


_INSTALLED = False


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    public = {
        "Axes": Axes,
        "ParametricFunction": ParametricFunction,
        "FunctionGraph": FunctionGraph,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
