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

    def input_to_graph_point(self, x: float, graph: ParametricFunction | _compat.VMobject) -> object:
        """Return the point on a graph corresponding to an axis x value.

        ManimCE v0.21 directly evaluates authored function graphs through their
        `function` callback. General VMobject lookup falls back to a path-space binary
        search upstream; Noon fails closed on that broader case until generic
        point-from-proportion semantics are shared.
        """

        if hasattr(graph, "underlying_function"):
            return graph.function(float(x))
        raise NotImplementedError(
            "input_to_graph_point for generic VMobjects requires shared point-from-proportion semantics"
        )

    def input_to_graph_coords(
        self, x: float, graph: ParametricFunction
    ) -> tuple[float, float]:
        return float(x), float(graph.underlying_function(float(x)))

    def i2gc(self, x: float, graph: ParametricFunction) -> tuple[float, float]:
        return self.input_to_graph_coords(x, graph)

    def i2gp(self, x: float, graph: ParametricFunction) -> object:
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
        """Construct a retained line from one axis to a scene-space point.

        Projection remains owned by the Rust-backed coordinate query path: convert the
        scene point to current transformed axis coordinates, zero the orthogonal
        coordinate, then map it back through `c2p`. This is equivalent to Manim's
        `NumberLine.get_projection` for the supported linear 2D Axes transform model.
        """

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

        # ManimCE v0.21 mutates the supplied line_config and lets these explicit
        # helper options override any same-named entries already present.
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
        """Draw a retained corner-path line graph using current transformed Axes state."""

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
        "VDict": VDict,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
    _INSTALLED = True