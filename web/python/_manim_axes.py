"""Initial ManimCE v0.21 linear Axes facade over shared Rust semantics.

The supported subset deliberately excludes tips and text/number labels until those
rendering paths are parity-qualified. Axis placement, ticks, coordinate queries,
plot sampling, smoothing, and path construction remain Rust-owned.
"""

from __future__ import annotations

import json
import math
from typing import Any, Callable, Sequence

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared

try:
    from js import noonCreateAuthoringMobjectHandle as _create_mobject_handle
    from js import noonCreateAxesAuthoringPlan as _create_axes_plan
    from js import noonCreateAxesPlotPlan as _create_plot_plan
    from js import noonCreateAxesQueryPlan as _create_query_plan
except ImportError:
    _create_mobject_handle = None
    _create_axes_plan = None
    _create_plot_plan = None
    _create_query_plan = None

_INSTALLED = False


def _require_shared_axes() -> None:
    if any(
        factory is None
        for factory in (
            _create_mobject_handle,
            _create_axes_plan,
            _create_plot_plan,
            _create_query_plan,
        )
    ):
        raise RuntimeError("Axes requires Noon's browser shared-semantics runtime")


def _range(value: Sequence[float] | None, default: tuple[float, float, float]) -> list[float]:
    if value is None:
        return list(default)
    values = [float(component) for component in value]
    if len(values) == 2:
        values.append(1.0)
    if len(values) != 3 or not all(math.isfinite(component) for component in values):
        raise ValueError("Axes ranges must contain 2 or 3 finite values")
    return values


def _rgba(value: object) -> list[float]:
    if not isinstance(value, _base.Color):
        raise TypeError("axis color must be a Noon/Manim Color")
    return [float(value.red), float(value.green), float(value.blue), float(value.alpha)]


def _axis_config(value: dict[str, Any] | None) -> dict[str, Any]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise TypeError("axis configuration must be a dict or None")
    result = dict(value)
    if "color" in result:
        result["color"] = _rgba(result["color"])
    if "numbers_with_elongated_ticks" in result:
        result["numbers_with_elongated_ticks"] = [
            float(number) for number in result["numbers_with_elongated_ticks"]
        ]
    return result


def _plain_copy(value: object) -> object:
    """Clone JSON-safe facade metadata without touching any WASM proxy."""

    return json.loads(json.dumps(value, separators=(",", ":"), allow_nan=False))


def _snapshot_handle(snapshot: dict[str, Any]):
    assert _create_mobject_handle is not None
    return _create_mobject_handle(json.dumps(snapshot, separators=(",", ":"), allow_nan=False))


def _mobject_from_snapshot(cls: type[_base.Mobject], snapshot: dict[str, Any]) -> _base.Mobject:
    value = object.__new__(cls)
    _shared._attach_shared_handle(value, _snapshot_handle(snapshot))
    return value


def _line_from_snapshot(snapshot: dict[str, Any]) -> _compat.Line:
    value = _mobject_from_snapshot(_compat.Line, snapshot)
    geometry = snapshot.get("geometry", {}).get("line")
    if not isinstance(geometry, dict):
        raise TypeError("shared Axes line snapshot is malformed")
    start = geometry["start"]
    end = geometry["end"]
    value.start = _base.Vec2(float(start["x"]), float(start["y"]))
    value.end = _base.Vec2(float(end["x"]), float(end["y"]))
    return value


class ParametricFunction(_compat.VMobject):
    """Source-visible retained curve type produced by :meth:`Axes.plot`.

    The current browser slice owns scalar Axes plots through the shared two-phase Rust
    plan. Direct ParametricFunction authoring needs the more general vector-valued
    callback plan and therefore fails explicitly rather than constructing a Python-only
    geometry path.
    """

    def __init__(self, *args: object, **kwargs: object) -> None:
        del args, kwargs
        raise NotImplementedError(
            "direct ParametricFunction construction is not yet supported by the shared browser plan"
        )

    def get_function(self) -> Callable[[float], object]:
        return self.function

    def get_point_from_function(self, value: float) -> object:
        return self.function(float(value))


class FunctionGraph(ParametricFunction):
    """Source-visible FunctionGraph type pending direct shared callback authoring."""


class _NumberLineFamily(_compat.VGroup):
    """Retained main line plus tick leaves for the initial NumberLine subset."""

    def __init__(self, wire: dict[str, Any]) -> None:
        self._line = _line_from_snapshot(wire["line"])
        self.ticks = _compat.VGroup(
            *(_line_from_snapshot(tick["snapshot"]) for tick in wire["ticks"])
        )
        super().__init__(self._line, self.ticks)

    def get_start(self) -> _base.Vec2:
        return self._line.get_start()

    def get_end(self) -> _base.Vec2:
        return self._line.get_end()

    def _axis_snapshot_json(self) -> str:
        handle = _shared._handle_for(self._line)
        if handle is None:
            raise RuntimeError("Axes main line no longer has an authoritative shared handle")
        return str(handle.snapshotJson())


class Axes(_compat.VGroup):
    """Shared linear 2D Axes subset with transform-safe c2p/p2c and plotting."""

    def __init__(
        self,
        x_range: Sequence[float] | None = None,
        y_range: Sequence[float] | None = None,
        x_length: float | None = 12.0,
        y_length: float | None = 6.0,
        axis_config: dict[str, Any] | None = None,
        x_axis_config: dict[str, Any] | None = None,
        y_axis_config: dict[str, Any] | None = None,
        tips: bool = True,
        **kwargs: Any,
    ) -> None:
        _require_shared_axes()
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Axes option(s): {unsupported}")
        if x_length is None or y_length is None:
            raise NotImplementedError("automatic Axes length resolution is not yet supported")

        self.x_range = _range(x_range, (-7.0, 7.0, 1.0))
        self.y_range = _range(y_range, (-4.0, 4.0, 1.0))
        self.x_length = float(x_length)
        self.y_length = float(y_length)
        self.axis_config = _axis_config(axis_config)
        self.x_axis_config = _axis_config(x_axis_config)
        self.y_axis_config = _axis_config(y_axis_config)
        self._base_request = {
            "x_range": self.x_range,
            "y_range": self.y_range,
            "x_length": self.x_length,
            "y_length": self.y_length,
        }
        request = {
            **self._base_request,
            "tips": bool(tips),
            "axis_config": self.axis_config,
            "x_axis_config": self.x_axis_config,
            "y_axis_config": self.y_axis_config,
        }
        assert _create_axes_plan is not None and _create_query_plan is not None
        self._axes_plan = _create_axes_plan(json.dumps(request, separators=(",", ":")))
        self._query_plan = _create_query_plan(
            json.dumps(self._base_request, separators=(",", ":"))
        )
        wire = json.loads(str(self._axes_plan.geometryJson()))
        self.x_axis = _NumberLineFamily(wire["x_axis"])
        self.y_axis = _NumberLineFamily(wire["y_axis"])
        self.axes = _compat.VGroup(self.x_axis, self.y_axis)
        super().__init__(*self.axes)

    def copy(self) -> Axes:
        """Clone retained axis families while sharing immutable Rust plans.

        Generic Group copying deep-copies subclass metadata. Pyodide WASM plans are
        immutable JsProxy values and must not cross that host-language deepcopy path.
        The copied leaves receive fresh shared semantic identities through their normal
        Group/Mobject copy adapters, while both Axes wrappers may safely reference the
        same immutable construction/query plans because every query supplies its own
        current retained line snapshots.
        """

        clone = object.__new__(type(self))
        clone.x_range = list(self.x_range)
        clone.y_range = list(self.y_range)
        clone.x_length = self.x_length
        clone.y_length = self.y_length
        clone.axis_config = _plain_copy(self.axis_config)
        clone.x_axis_config = _plain_copy(self.x_axis_config)
        clone.y_axis_config = _plain_copy(self.y_axis_config)
        clone._base_request = _plain_copy(self._base_request)
        clone._axes_plan = self._axes_plan
        clone._query_plan = self._query_plan
        clone.x_axis = self.x_axis.copy()
        clone.y_axis = self.y_axis.copy()
        clone.axes = _compat.VGroup(clone.x_axis, clone.y_axis)
        _compat.VGroup.__init__(clone, clone.x_axis, clone.y_axis)
        return clone

    def get_axes(self) -> _compat.VGroup:
        return self.axes

    def get_axis(self, index: int) -> _NumberLineFamily:
        return self.axes[index]

    def get_x_axis(self) -> _NumberLineFamily:
        return self.x_axis

    def get_y_axis(self) -> _NumberLineFamily:
        return self.y_axis

    def coords_to_point(self, *coords: float) -> _base.Vec2:
        if len(coords) < 2:
            raise TypeError("Axes.coords_to_point requires x and y coordinates")
        if len(coords) > 3 or (len(coords) == 3 and not math.isclose(float(coords[2]), 0.0)):
            raise NotImplementedError("Noon Axes currently supports 2D coordinates only")
        result = json.loads(
            str(
                self._query_plan.coordsToPointJson(
                    float(coords[0]),
                    float(coords[1]),
                    self.x_axis._axis_snapshot_json(),
                    self.y_axis._axis_snapshot_json(),
                )
            )
        )
        return _base.Vec2(float(result[0]), float(result[1]))

    c2p = coords_to_point

    def point_to_coords(self, point: object) -> list[float]:
        value = _compat._as_vec2(point)
        result = json.loads(
            str(
                self._query_plan.pointToCoordsJson(
                    float(value.x),
                    float(value.y),
                    self.x_axis._axis_snapshot_json(),
                    self.y_axis._axis_snapshot_json(),
                )
            )
        )
        return [float(result[0]), float(result[1])]

    p2c = point_to_coords

    def get_origin(self) -> _base.Vec2:
        return self.coords_to_point(0.0, 0.0)

    def __matmul__(self, coord: object) -> _base.Vec2:
        if isinstance(coord, (_base.Mobject, _compat.Group)):
            coord = coord.get_center()
        try:
            return self.coords_to_point(*coord)  # type: ignore[arg-type]
        except TypeError as error:
            raise TypeError("Axes @ value expects a coordinate sequence or Mobject") from error

    def __rmatmul__(self, point: object) -> list[float]:
        return self.point_to_coords(point)

    def plot(
        self,
        function: Callable[[float], float],
        x_range: Sequence[float] | None = None,
        use_smoothing: bool = True,
        discontinuities: Sequence[float] | None = None,
        dt: float = 1.0e-8,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> ParametricFunction:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Axes.plot option(s): {unsupported}")
        if not callable(function):
            raise TypeError("Axes.plot function must be callable")
        request = {
            **self._base_request,
            "plot_range": None if x_range is None else [float(value) for value in x_range],
            "discontinuities": (
                None
                if discontinuities is None
                else [float(value) for value in discontinuities]
            ),
            "discontinuity_dt": float(dt),
            "use_smoothing": bool(use_smoothing),
        }
        assert _create_plot_plan is not None
        plan = _create_plot_plan(json.dumps(request, separators=(",", ":"), allow_nan=False))
        parameters = json.loads(str(plan.parametersJson()))
        values = [
            [float(function(float(parameter))) for parameter in subpath]
            for subpath in parameters
        ]
        snapshot_json = str(
            plan.finishSnapshotJsonWithAxes(
                json.dumps(values, separators=(",", ":"), allow_nan=False),
                self.x_axis._axis_snapshot_json(),
                self.y_axis._axis_snapshot_json(),
            )
        )
        graph = object.__new__(ParametricFunction)
        assert _create_mobject_handle is not None
        _shared._attach_shared_handle(graph, _create_mobject_handle(snapshot_json))
        graph.function = lambda value: self.coords_to_point(float(value), function(float(value)))
        graph.underlying_function = function
        graph.x_range = self.x_range if x_range is None else list(x_range)
        graph.axes = self
        if color is not None:
            graph.set_color(color)
        return graph


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
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
    _INSTALLED = True
