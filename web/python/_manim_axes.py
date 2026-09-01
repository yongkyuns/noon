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
    from js import noonCreateParametricFunctionPlan as _create_parametric_plan
except ImportError:
    _create_mobject_handle = None
    _create_axes_plan = None
    _create_plot_plan = None
    _create_query_plan = None
    _create_parametric_plan = None

_INSTALLED = False
_DEFAULT_LINE_GRAPH_COLOR = getattr(
    _base, "PURE_YELLOW", _base.color_from_hex("#FFFF00")
)
_DEFAULT_DOT_RADIUS = float(getattr(_base, "DEFAULT_DOT_RADIUS", 0.08))


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


def _require_shared_parametric() -> None:
    if _create_mobject_handle is None or _create_parametric_plan is None:
        raise RuntimeError(
            "ParametricFunction requires Noon's browser shared-semantics runtime"
        )


def _range(value: Sequence[float] | None, default: tuple[float, float, float]) -> list[float]:
    if value is None:
        return list(default)
    values = [float(component) for component in value]
    if len(values) == 2:
        values.append(1.0)
    if len(values) != 3 or not all(math.isfinite(component) for component in values):
        raise ValueError("Axes ranges must contain 2 or 3 finite values")
    return values


def _parametric_range(value: Sequence[float] | None) -> list[float]:
    values = [0.0, 1.0, 0.01] if value is None else [float(component) for component in value]
    if len(values) == 2:
        values.append(0.01)
    if len(values) != 3 or not all(math.isfinite(component) for component in values):
        raise ValueError("t_range must contain 2 or 3 finite values")
    return values


def _parametric_coordinates(function: Callable[[float], object], parameter: float) -> list[float]:
    value = function(float(parameter))
    try:
        coordinates = value[:2]  # type: ignore[index]
    except (TypeError, IndexError) as error:
        raise TypeError("parametric function must return at least two coordinates") from error
    if len(coordinates) < 2:
        raise ValueError("parametric function must return at least two coordinates")
    return [float(coordinates[0]), float(coordinates[1])]


def _scene_parametric_coordinates(
    function: Callable[[float], object], parameter: float
) -> list[float]:
    value = function(float(parameter))
    try:
        coordinates = list(value)  # type: ignore[arg-type]
    except TypeError as error:
        raise TypeError("ParametricFunction callback must return coordinates") from error
    if len(coordinates) < 2:
        raise ValueError("ParametricFunction callback must return at least two coordinates")
    x = float(coordinates[0])
    y = float(coordinates[1])
    if not math.isfinite(x) or not math.isfinite(y):
        raise ValueError("ParametricFunction callback coordinates must be finite")
    if len(coordinates) >= 3:
        z = float(coordinates[2])
        if not math.isfinite(z):
            raise ValueError("ParametricFunction callback coordinates must be finite")
        if not math.isclose(z, 0.0, abs_tol=1.0e-12):
            raise NotImplementedError(
                "direct ParametricFunction with nonzero z requires the retained 3D geometry path"
            )
    return [x, y]


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


def _vector_path_vertices(snapshot: dict[str, Any]) -> list[_base.Vec2]:
    geometry = snapshot.get("geometry", {}).get("vector_path")
    if not isinstance(geometry, dict):
        raise TypeError("shared Axes line-graph snapshot is malformed")
    commands = geometry.get("commands")
    if not isinstance(commands, list):
        raise TypeError("shared Axes line-graph path commands are malformed")

    vertices: list[_base.Vec2] = []
    for command in commands:
        if not isinstance(command, dict):
            raise TypeError("shared Axes line-graph path command is malformed")
        payload = command.get("move_to") or command.get("line_to")
        if not isinstance(payload, dict) or not isinstance(payload.get("to"), dict):
            raise TypeError("Axes line graph must contain only corner path commands")
        point = payload["to"]
        vertices.append(_base.Vec2(float(point["x"]), float(point["y"])))
    return vertices


def _color(name: str, value: object) -> _base.Color:
    return _shared._phase_b._as_color(name, value)


def _invert_color(value: _base.Color) -> _base.Color:
    """ManimColor.invert(with_alpha=False): linearly invert RGB, preserve alpha."""

    return _base.Color(
        1.0 - float(value.red),
        1.0 - float(value.green),
        1.0 - float(value.blue),
        float(value.alpha),
    )


def _color_gradient(reference_colors: object, length: int) -> list[_base.Color]:
    """Pinned ManimCE v0.21 ``color_gradient`` for per-rectangle colors."""

    if length == 0:
        return []
    raw = list(reference_colors) if isinstance(reference_colors, (list, tuple)) else [reference_colors]
    colors = [_color("color", value) for value in raw]
    if not colors:
        raise ValueError("Expected 1 or more reference colors. Got 0 colors.")
    if len(colors) == 1:
        return colors * length
    if length == 1:
        return [colors[-1]]

    result: list[_base.Color] = []
    last = len(colors) - 1
    for index in range(length):
        scaled = last * index / (length - 1)
        floor = min(int(scaled), last - 1)
        alpha = scaled - floor
        if index == length - 1:
            floor = last - 1
            alpha = 1.0
        start = colors[floor]
        end = colors[floor + 1]
        result.append(
            _base.Color(
                float(start.red) * (1.0 - alpha) + float(end.red) * alpha,
                float(start.green) * (1.0 - alpha) + float(end.green) * alpha,
                float(start.blue) * (1.0 - alpha) + float(end.blue) * alpha,
                1.0,
            )
        )
    return result


def _graph_range(graph: object) -> list[float]:
    if hasattr(graph, "t_min") and hasattr(graph, "t_max"):
        return [float(graph.t_min), float(graph.t_max)]
    values = getattr(graph, "x_range", None)
    if values is None or len(values) < 2:
        raise TypeError("Axes calculus helpers require an authored graph range")
    return [float(values[0]), float(values[1])]


def _authored_graph_function(graph: object, name: str) -> Callable[[float], float]:
    function = getattr(graph, "underlying_function", None)
    if not callable(function):
        raise NotImplementedError(
            f"{name} currently requires an Axes.plot authored function graph"
        )
    return function


def _graph_snapshot_json(graph: object, name: str) -> str:
    handle = _shared._handle_for(graph)
    if handle is None or not hasattr(handle, "snapshotJson"):
        raise NotImplementedError(f"{name} requires current retained graph geometry")
    return str(handle.snapshotJson())


class ParametricFunction(_compat.VMobject):
    """Retained ManimCE v0.21 scene-space parametric curve for the 2D renderer."""

    def __init__(
        self,
        function: Callable[[float], object],
        t_range: Sequence[float] = (0.0, 1.0),
        scaling: object | None = None,
        dt: float = 1.0e-8,
        discontinuities: Sequence[float] | None = None,
        use_smoothing: bool = True,
        use_vectorized: bool = False,
        **kwargs: Any,
    ) -> None:
        _require_shared_parametric()
        if not callable(function):
            raise TypeError("ParametricFunction function must be callable")
        if scaling is not None:
            raise NotImplementedError(
                "nonlinear ParametricFunction scaling is not yet supported"
            )
        if use_vectorized:
            raise NotImplementedError(
                "ParametricFunction(use_vectorized=True) requires vectorized callback transport"
            )
        color = kwargs.pop("color", None)
        resolved_range = _parametric_range(t_range)
        request = {
            "t_range": resolved_range,
            "discontinuities": (
                None
                if discontinuities is None
                else [float(value) for value in discontinuities]
            ),
            "discontinuity_dt": float(dt),
            "use_smoothing": bool(use_smoothing),
        }
        assert _create_parametric_plan is not None
        plan = _create_parametric_plan(
            json.dumps(request, separators=(",", ":"), allow_nan=False)
        )
        parameters = json.loads(str(plan.parametersJson()))
        values = [
            [_scene_parametric_coordinates(function, parameter) for parameter in subpath]
            for subpath in parameters
        ]
        snapshot_json = str(
            plan.finishSnapshotJson(
                json.dumps(values, separators=(",", ":"), allow_nan=False)
            )
        )
        assert _create_mobject_handle is not None
        _shared._attach_shared_handle(self, _create_mobject_handle(snapshot_json))
        self.function = function
        self.scaling = scaling
        self.dt = float(dt)
        self.discontinuities = discontinuities
        self.use_smoothing = bool(use_smoothing)
        self.use_vectorized = False
        self.t_range = list(resolved_range)
        self.t_min = float(resolved_range[0])
        self.t_max = float(resolved_range[1])
        self.t_step = float(resolved_range[2])
        _shared._apply_shared_constructor_kwargs(
            self,
            _compat._manim_vmobject_kwargs(kwargs, default_color=_base.WHITE),
        )
        if color is not None:
            self.set_color(_color("color", color))

    def get_function(self) -> Callable[[float], object]:
        return self.function

    def get_point_from_function(self, value: float) -> object:
        return self.function(float(value))


class FunctionGraph(ParametricFunction):
    """Retained v0.21 scalar function graph in scene coordinates."""

    def __init__(
        self,
        function: Callable[[float], object],
        x_range: Sequence[float] | None = None,
        color: object = _DEFAULT_LINE_GRAPH_COLOR,
        **kwargs: Any,
    ) -> None:
        if not callable(function):
            raise TypeError("FunctionGraph function must be callable")
        resolved_range = (
            [-float(_base.DEFAULT_FRAME_WIDTH) / 2.0, float(_base.DEFAULT_FRAME_WIDTH) / 2.0]
            if x_range is None
            else [float(value) for value in x_range]
        )
        if len(resolved_range) not in (2, 3):
            raise ValueError("FunctionGraph x_range must contain 2 or 3 finite values")
        self.x_range = list(resolved_range)
        self.underlying_function = function
        self.parametric_function = lambda value: [
            float(value),
            float(function(float(value))),
            0.0,
        ]
        super().__init__(
            self.parametric_function,
            t_range=self.x_range,
            color=_color("color", color),
            **kwargs,
        )

    def get_function(self) -> Callable[[float], object]:
        return self.underlying_function

    def get_point_from_function(self, value: float) -> object:
        return self.parametric_function(float(value))


class VDict(_compat.VGroup):
    """Flat retained family with ManimCE v0.21 dictionary-style authoring access.

    Keys are authoring metadata only. Runtime membership remains the ordinary retained
    leaf family already used by :class:`VGroup`; no dictionary node is serialized.
    """

    def __init__(
        self,
        mapping_or_iterable: object = None,
        show_keys: bool = False,
        **kwargs: Any,
    ) -> None:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported VDict option(s): {unsupported}")
        if show_keys:
            raise NotImplementedError("VDict(show_keys=True) requires exact Tex rendering")
        self.show_keys = False
        self.submob_dict: dict[object, object] = {}
        _compat.VGroup.__init__(self)
        if mapping_or_iterable is not None:
            self.add(mapping_or_iterable)

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({self.submob_dict!r})"

    def add(self, mapping_or_iterable: object = None) -> VDict:
        if mapping_or_iterable is None:
            return self
        try:
            items = dict(mapping_or_iterable).items()  # type: ignore[arg-type]
        except (TypeError, ValueError) as error:
            raise TypeError("VDict.add expects a mapping or iterable of key/Mobject pairs") from error
        for key, value in items:
            self.add_key_value_pair(key, value)
        return self

    def add_key_value_pair(self, key: object, value: object) -> VDict:
        if not isinstance(value, (_base.Mobject, _compat.Group)):
            raise TypeError("VDict values must be Mobjects or Groups")
        self.submob_dict[key] = value
        _compat.VGroup.add(self, value)
        return self

    def remove(self, key: object) -> VDict:
        if key not in self.submob_dict:
            raise KeyError(f"The given key '{key!s}' is not present in the VDict")
        value = self.submob_dict[key]
        _compat.VGroup.remove(self, value)
        del self.submob_dict[key]
        return self

    def __getitem__(self, key: object) -> object:
        return self.submob_dict[key]

    def __setitem__(self, key: object, value: object) -> None:
        if key in self.submob_dict:
            self.remove(key)
        self.add([(key, value)])

    def __delitem__(self, key: object) -> None:
        del self.submob_dict[key]

    def __contains__(self, key: object) -> bool:
        return key in self.submob_dict

    def get_all_submobjects(self):
        return self.submob_dict.values()

    def copy(self) -> VDict:
        return type(self)(
            [(key, value.copy()) for key, value in self.submob_dict.items()]
        )


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
        """Clone retained axis families while sharing immutable Rust plans."""
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

    def input_to_graph_point(
        self, x: float, graph: ParametricFunction | _compat.VMobject
    ) -> object:
        value = float(x)
        if hasattr(graph, "underlying_function"):
            return graph.function(value)
        handle = _shared._handle_for(graph)
        if handle is None:
            raise TypeError(
                "input_to_graph_point requires an authored graph or retained VMobject"
            )
        try:
            result = json.loads(
                str(
                    self._query_plan.graphPointForXJson(
                        value,
                        str(handle.snapshotJson()),
                        self.x_axis._axis_snapshot_json(),
                        self.y_axis._axis_snapshot_json(),
                    )
                )
            )
        except Exception as error:
            raise ValueError(str(error)) from None
        return _base.Vec2(float(result[0]), float(result[1]))

    def input_to_graph_coords(
        self, x: float, graph: ParametricFunction
    ) -> tuple[float, float]:
        return float(x), float(graph.underlying_function(float(x)))

    def i2gc(self, x: float, graph: ParametricFunction) -> tuple[float, float]:
        return self.input_to_graph_coords(x, graph)

    def i2gp(self, x: float, graph: ParametricFunction | _compat.VMobject) -> object:
        return self.input_to_graph_point(x, graph)

    def get_line_from_axis_to_point(
        self,
        index: int,
        point: object,
        line_func: Callable[..., _compat.Line] | None = None,
        line_config: dict[str, Any] | None = None,
        color: _base.Color | None = None,
        stroke_width: float = 2.0,
    ) -> _compat.Line:
        if index not in (0, 1):
            raise IndexError("Noon Axes currently has only x and y axes")
        target = _compat._as_vec2(point)
        coords = self.p2c(target)
        projected = self.c2p(coords[0], 0.0) if index == 0 else self.c2p(0.0, coords[1])
        if line_func is None:
            line_func = getattr(_base, "DashedLine", None)
            if line_func is None:
                raise NotImplementedError(
                    "default dashed axis helper lines require the public DashedLine facade"
                )
        if not callable(line_func):
            raise TypeError("line_func must construct a Line-compatible mobject")
        if line_config is None:
            line_config = {}
        elif not isinstance(line_config, dict):
            raise TypeError("line_config must be a dict or None")
        line_config["color"] = _base.WHITE if color is None else color
        line_config["stroke_width"] = float(stroke_width)
        return line_func(projected, target, **line_config)

    def get_vertical_line(self, point: object, **kwargs: Any) -> _compat.Line:
        return self.get_line_from_axis_to_point(0, point, **kwargs)

    def get_horizontal_line(self, point: object, **kwargs: Any) -> _compat.Line:
        return self.get_line_from_axis_to_point(1, point, **kwargs)

    def plot_line_graph(
        self,
        x_values: object,
        y_values: object,
        z_values: object | None = None,
        line_color: _base.Color = _DEFAULT_LINE_GRAPH_COLOR,
        add_vertex_dots: bool = True,
        vertex_dot_radius: float = _DEFAULT_DOT_RADIUS,
        vertex_dot_style: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> VDict:
        if not isinstance(line_color, _base.Color):
            raise TypeError("line_color must be a Noon/Manim Color")
        try:
            xs = [float(value) for value in x_values]  # type: ignore[union-attr]
            ys = [float(value) for value in y_values]  # type: ignore[union-attr]
        except (TypeError, ValueError) as error:
            raise TypeError("x_values and y_values must be numeric iterables") from error
        if z_values is None:
            zs = [0.0] * len(xs)
        else:
            try:
                zs = [float(value) for value in z_values]  # type: ignore[union-attr]
            except (TypeError, ValueError) as error:
                raise TypeError("z_values must be a numeric iterable or None") from error
        if len(xs) != len(ys) or len(xs) != len(zs):
            raise ValueError("x_values, y_values, and z_values must have equal lengths")
        if not all(math.isfinite(value) for value in (*xs, *ys, *zs)):
            raise ValueError("line-graph coordinates must be finite")
        if any(not math.isclose(value, 0.0, abs_tol=1.0e-12) for value in zs):
            raise NotImplementedError("Noon Axes.plot_line_graph currently supports z=0 only")
        if vertex_dot_style is None:
            dot_style: dict[str, Any] = {}
        elif isinstance(vertex_dot_style, dict):
            dot_style = dict(vertex_dot_style)
        else:
            raise TypeError("vertex_dot_style must be a dict or None")
        snapshot_json = str(
            self._query_plan.lineGraphSnapshotJson(
                json.dumps([xs, ys], separators=(",", ":"), allow_nan=False),
                self.x_axis._axis_snapshot_json(),
                self.y_axis._axis_snapshot_json(),
            )
        )
        snapshot = json.loads(snapshot_json)
        graph = _mobject_from_snapshot(_compat.VMobject, snapshot)
        _shared._apply_shared_constructor_kwargs(
            graph,
            _compat._manim_vmobject_kwargs(kwargs, default_color=line_color),
        )
        result = VDict([("line_graph", graph)])
        if add_vertex_dots:
            vertices = _vector_path_vertices(snapshot)
            vertex_dots = _compat.VGroup(
                *(
                    _base.Dot(
                        point=vertex,
                        radius=float(vertex_dot_radius),
                        **dot_style,
                    )
                    for vertex in vertices
                )
            )
            result["vertex_dots"] = vertex_dots
        return result

    def get_riemann_rectangles(
        self,
        graph: ParametricFunction,
        x_range: Sequence[float] | None = None,
        dx: float = 0.1,
        input_sample_type: str = "left",
        stroke_width: float = 1.0,
        stroke_color: object = _base.BLACK,
        fill_opacity: float = 1.0,
        color: object = (_base.BLUE, _base.GREEN),
        show_signed_area: bool = True,
        bounded_graph: ParametricFunction | None = None,
        blend: bool = False,
        width_scale_factor: float = 1.001,
    ) -> _compat.VGroup:
        graph_function = _authored_graph_function(graph, "get_riemann_rectangles")
        bounded_function = (
            None
            if bounded_graph is None
            else _authored_graph_function(bounded_graph, "get_riemann_rectangles")
        )
        explicit_range = None
        if x_range is not None:
            values = [float(value) for value in x_range]
            if len(values) < 2:
                raise ValueError("x_range must contain at least two values")
            explicit_range = values[:2]
        request = {
            "graph_range": _graph_range(graph),
            "bounded_graph_range": (
                None if bounded_graph is None else _graph_range(bounded_graph)
            ),
            "x_range": explicit_range,
            "dx": float(dx),
            "input_sample_type": str(input_sample_type),
            "width_scale_factor": float(width_scale_factor),
        }
        request_json = json.dumps(request, separators=(",", ":"), allow_nan=False)
        try:
            sample_values = json.loads(
                str(
                    self._query_plan.riemannSampleValuesJson(
                        request_json,
                        self.x_axis._axis_snapshot_json(),
                        self.y_axis._axis_snapshot_json(),
                    )
                )
            )
        except Exception as error:
            message = str(error)
            if message.removeprefix("Error: ") == "Invalid input sample type":
                raise ValueError("Invalid input sample type") from None
            raise
        graph_y_values = [float(graph_function(float(x))) for x in sample_values["graph"]]
        bounded_x_values = sample_values.get("bounded_graph")
        bounded_y_values = (
            None
            if bounded_function is None
            else [float(bounded_function(float(x))) for x in bounded_x_values]
        )
        result = json.loads(
            str(
                self._query_plan.riemannRectanglesJson(
                    request_json,
                    json.dumps(graph_y_values, separators=(",", ":"), allow_nan=False),
                    (
                        ""
                        if bounded_y_values is None
                        else json.dumps(
                            bounded_y_values,
                            separators=(",", ":"),
                            allow_nan=False,
                        )
                    ),
                    self.x_axis._axis_snapshot_json(),
                    self.y_axis._axis_snapshot_json(),
                )
            )
        )
        colors = _color_gradient(color, len(result))
        stroke = _color("stroke_color", stroke_color)
        rectangles: list[_base.Mobject] = []
        for wire, fill_color in zip(result, colors, strict=True):
            if bool(wire["negative_signed_area"]) and bool(show_signed_area):
                fill_color = _invert_color(fill_color)
            rectangle = _mobject_from_snapshot(_compat.Rectangle, wire["snapshot"])
            _shared._apply_shared_constructor_kwargs(
                rectangle,
                {
                    "fill_color": fill_color,
                    "fill_opacity": float(fill_opacity),
                    "stroke_color": fill_color if blend else stroke,
                    "stroke_width": float(stroke_width),
                },
            )
            rectangles.append(rectangle)
        return _compat.VGroup(*rectangles)

    def get_area(
        self,
        graph: ParametricFunction,
        x_range: Sequence[float] | None = None,
        color: object = (_base.BLUE, _base.GREEN),
        opacity: float = 0.3,
        bounded_graph: ParametricFunction | None = None,
        **kwargs: Any,
    ) -> _compat.VMobject:
        graph_function = _authored_graph_function(graph, "get_area")
        bounded_function = (
            None if bounded_graph is None else _authored_graph_function(bounded_graph, "get_area")
        )
        if isinstance(color, (list, tuple)):
            raise NotImplementedError(
                "gradient-filled Axes.get_area requires retained gradient-fill support"
            )
        explicit_range = None
        if x_range is not None:
            values = [float(value) for value in x_range]
            if len(values) != 2:
                raise ValueError("x_range must contain exactly two values")
            explicit_range = values
        graph_snapshot_json = _graph_snapshot_json(graph, "get_area")
        bounded_snapshot_json = (
            "" if bounded_graph is None else _graph_snapshot_json(bounded_graph, "get_area")
        )
        request = {
            "graph_range": _graph_range(graph),
            "bounded_graph_range": (
                None if bounded_graph is None else _graph_range(bounded_graph)
            ),
            "x_range": explicit_range,
        }
        request_json = json.dumps(request, separators=(",", ":"), allow_nan=False)
        try:
            endpoints = json.loads(
                str(
                    self._query_plan.areaEndpointXValuesJson(
                        request_json,
                        graph_snapshot_json,
                        bounded_snapshot_json,
                        self.x_axis._axis_snapshot_json(),
                        self.y_axis._axis_snapshot_json(),
                    )
                )
            )
        except Exception as error:
            raise ValueError(str(error)) from None
        graph_y_values = [float(graph_function(float(x))) for x in endpoints]
        bounded_y_values = (
            None
            if bounded_function is None
            else [float(bounded_function(float(x))) for x in endpoints]
        )
        snapshot = json.loads(
            str(
                self._query_plan.areaSnapshotJson(
                    request_json,
                    graph_snapshot_json,
                    bounded_snapshot_json,
                    json.dumps(graph_y_values, separators=(",", ":"), allow_nan=False),
                    (
                        ""
                        if bounded_y_values is None
                        else json.dumps(
                            bounded_y_values,
                            separators=(",", ":"),
                            allow_nan=False,
                        )
                    ),
                    self.x_axis._axis_snapshot_json(),
                    self.y_axis._axis_snapshot_json(),
                )
            )
        )
        area = _mobject_from_snapshot(_compat.VMobject, snapshot)
        _shared._apply_shared_constructor_kwargs(
            area,
            _compat._manim_vmobject_kwargs(kwargs, default_color=_base.BLUE),
        )
        area.set_opacity(float(opacity))
        area.set_color(_color("color", color))
        return area

    def plot_parametric_curve(
        self,
        function: Callable[[float], object],
        use_vectorized: bool = False,
        **kwargs: Any,
    ) -> ParametricFunction:
        if not callable(function):
            raise TypeError("Axes.plot_parametric_curve function must be callable")
        if use_vectorized:
            raise NotImplementedError(
                "Axes.plot_parametric_curve(use_vectorized=True) requires vectorized callback transport"
            )
        t_range = _parametric_range(kwargs.pop("t_range", None))
        discontinuities = kwargs.pop("discontinuities", None)
        dt = float(kwargs.pop("dt", 1.0e-8))
        use_smoothing = bool(kwargs.pop("use_smoothing", True))
        color = kwargs.pop("color", None)
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"unsupported Axes.plot_parametric_curve option(s): {unsupported}"
            )
        request = {
            **self._base_request,
            "plot_range": t_range,
            "discontinuities": (
                None
                if discontinuities is None
                else [float(value) for value in discontinuities]
            ),
            "discontinuity_dt": dt,
            "use_smoothing": use_smoothing,
        }
        assert _create_plot_plan is not None
        plan = _create_plot_plan(json.dumps(request, separators=(",", ":"), allow_nan=False))
        parameters = json.loads(str(plan.parametersJson()))
        values = [
            [_parametric_coordinates(function, parameter) for parameter in subpath]
            for subpath in parameters
        ]
        snapshot_json = str(
            plan.finishParametricSnapshotJsonWithAxes(
                json.dumps(values, separators=(",", ":"), allow_nan=False),
                self.x_axis._axis_snapshot_json(),
                self.y_axis._axis_snapshot_json(),
            )
        )
        graph = object.__new__(ParametricFunction)
        assert _create_mobject_handle is not None
        _shared._attach_shared_handle(graph, _create_mobject_handle(snapshot_json))
        graph.function = lambda value: self.coords_to_point(
            *_parametric_coordinates(function, float(value))
        )
        graph.underlying_function = function
        graph.t_range = list(t_range)
        graph.t_min = float(t_range[0])
        graph.t_max = float(t_range[1])
        graph.axes = self
        if color is not None:
            graph.set_color(_color("color", color))
        return graph

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
        graph.t_min = float(graph.x_range[0])
        graph.t_max = float(graph.x_range[1])
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
        "VDict": VDict,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
    _INSTALLED = True
