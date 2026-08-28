"""Thin Manim geometry constructor adapters backed by shared Rust semantics.

This module intentionally patches only constructors whose full observable geometry/layout
contract is already owned by Rust. Class identity and inheritance remain unchanged where
an established compatibility class already exists; Arc is defined here because no legacy
Python Arc class predates the shared Rust implementation.
"""

from __future__ import annotations

import math
import operator
import warnings
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_geometry as _geometry
import _manim_semantic_handles as _shared

try:
    from js import noonCreateAuthoringDotHandle as _create_dot_handle
except ImportError:  # Native CPython tests keep the existing Python constructor.
    _create_dot_handle = None

try:
    from js import noonCreateAuthoringTriangleHandle as _create_triangle_handle
except ImportError:  # Native CPython tests keep the existing Python constructor.
    _create_triangle_handle = None

try:
    from js import noonCreateAuthoringArcSpec as _create_arc_spec
except ImportError:  # Native CPython keeps a non-authoritative compatibility fallback.
    _create_arc_spec = None

try:
    from js import noonCreateAuthoringArcBetweenPointsSpec as _create_arc_between_points_spec
except ImportError:
    _create_arc_between_points_spec = None

try:
    from js import noonQueryAuthoringArc as _query_arc
except ImportError:
    _query_arc = None

_ORIGINAL_DOT_INIT = _geometry.Dot.__init__
_ORIGINAL_TRIANGLE_INIT = _geometry.Triangle.__init__
_INSTALLED = False


def _set_shared_color(self: _base.Mobject, color: object) -> None:
    """Apply Manim ``color`` through the shared semantic handle.

    The generic Phase-B setter rebuilds a Python snapshot before handing it back to
    Rust. Shared constructors must not re-enter that compatibility path: parse the
    host color once, then use the semantic-handle mutation that preserves each
    channel's existing opacity.
    """

    parsed = _shared._phase_b._as_color("color", color)
    _shared._set_color(self, parsed)


def _dot_init(
    self: _geometry.Dot,
    point: object = _base.ORIGIN,
    radius: float = _geometry.DEFAULT_DOT_RADIUS,
    stroke_width: float = 0.0,
    fill_opacity: float = 1.0,
    color: _base.Color = _base.WHITE,
    **kwargs: Any,
) -> None:
    if _create_dot_handle is None:
        _ORIGINAL_DOT_INIT(
            self,
            point=point,
            radius=radius,
            stroke_width=stroke_width,
            fill_opacity=fill_opacity,
            color=color,
            **kwargs,
        )
        return

    point_value = _compat._as_vec2(point)
    radius_value = _shared._ir._positive_number("radius", radius)
    _shared._attach_shared_handle(
        self,
        _create_dot_handle(point_value.x, point_value.y, radius_value),
    )
    self.radius = radius_value

    options = dict(kwargs)
    options["stroke_width"] = stroke_width
    options["fill_opacity"] = fill_opacity
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


def _triangle_init(self: _geometry.Triangle, **kwargs: Any) -> None:
    if _create_triangle_handle is None:
        _ORIGINAL_TRIANGLE_INIT(self, **kwargs)
        return

    options = dict(kwargs)
    color = options.pop("color", None)
    _shared._attach_shared_handle(self, _create_triangle_handle())
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


def _arc_num_components(value: object) -> int:
    if isinstance(value, bool):
        raise TypeError("num_components must be an integer")
    try:
        result = operator.index(value)
    except TypeError as error:
        raise TypeError("num_components must be an integer") from error
    if result < 2:
        raise ValueError("num_components must be at least 2")
    if result > 0xFFFFFFFF:
        raise ValueError("num_components is too large")
    return int(result)


def _arc_number(name: str, value: object) -> float:
    return _shared._ir._finite_number(name, value)


def _fallback_arc_path(
    radius: float,
    start_angle: float,
    angle: float,
    num_components: int,
    center: _base.Vec2,
) -> _base.VectorPath:
    """Native-CPython-only mirror of the shared cubic Arc constructor."""

    segment_count = num_components - 1
    delta = angle / segment_count
    handle_factor = (4.0 / 3.0) * math.tan(delta / 4.0)

    def point_at(theta: float) -> _base.Vec2:
        return center + radius * _base.Vec2(math.cos(theta), math.sin(theta))

    def tangent_at(theta: float) -> _base.Vec2:
        return radius * _base.Vec2(-math.sin(theta), math.cos(theta))

    path = _base.VectorPath().move_to(point_at(start_angle))
    for index in range(segment_count):
        theta0 = start_angle + index * delta
        theta1 = theta0 + delta
        anchor0 = point_at(theta0)
        anchor1 = point_at(theta1)
        path = path.cubic_to(
            anchor0 + handle_factor * tangent_at(theta0),
            anchor1 - handle_factor * tangent_at(theta1),
            anchor1,
        )
    return path


def _fallback_arc_between_points(
    start: _base.Vec2,
    end: _base.Vec2,
    angle: float,
    radius: float | None,
    num_components: int,
) -> tuple[_base.VectorPath, float, float]:
    """Native-only ArcBetweenPoints construction matching the shared Rust algorithm."""

    chord = end - start
    chord_length = chord.length()
    radius_was_explicit = radius is not None
    if radius is None:
        base_radius = 1.0
        resolved_angle = angle
    else:
        sign = -2.0 if radius < 0.0 else 2.0
        base_radius = abs(radius)
        half_distance = chord_length * 0.5
        if base_radius < half_distance:
            raise ValueError(
                f"ArcBetweenPoints radius {base_radius} is smaller than half the endpoint distance {half_distance}"
            )
        if base_radius == 0.0:
            raise ValueError("ArcBetweenPoints radius cannot resolve a degenerate chord")
        adjacent = math.sqrt(max(base_radius * base_radius - half_distance * half_distance, 0.0))
        resolved_angle = math.acos(adjacent / base_radius) * sign

    if resolved_angle == 0.0:
        path = _base.VectorPath().move_to(start).line_to(end)
        resolved_radius = base_radius if radius_was_explicit else math.inf
        return path, resolved_radius, resolved_angle

    base_start = _base.Vec2(base_radius, 0.0)
    base_end = base_radius * _base.Vec2(math.cos(resolved_angle), math.sin(resolved_angle))
    base_chord = base_end - base_start
    base_chord_length = base_chord.length()
    if base_chord_length <= 1e-12:
        raise ValueError("ArcBetweenPoints angle has a zero-length source chord")

    scale = chord_length / base_chord_length
    rotation = math.atan2(chord.y, chord.x) - math.atan2(base_chord.y, base_chord.x)
    cosine = math.cos(rotation)
    sine = math.sin(rotation)

    def transform(point: _base.Vec2) -> _base.Vec2:
        local = (point - base_start) * scale
        return start + _base.Vec2(
            local.x * cosine - local.y * sine,
            local.x * sine + local.y * cosine,
        )

    segment_count = num_components - 1
    delta = resolved_angle / segment_count
    handle_factor = (4.0 / 3.0) * math.tan(delta / 4.0)

    def point_at(theta: float) -> _base.Vec2:
        return base_radius * _base.Vec2(math.cos(theta), math.sin(theta))

    def tangent_at(theta: float) -> _base.Vec2:
        return base_radius * _base.Vec2(-math.sin(theta), math.cos(theta))

    path = _base.VectorPath().move_to(start)
    for index in range(segment_count):
        theta0 = index * delta
        theta1 = theta0 + delta
        anchor0 = point_at(theta0)
        anchor1 = point_at(theta1)
        path = path.cubic_to(
            transform(anchor0 + handle_factor * tangent_at(theta0)),
            transform(anchor1 - handle_factor * tangent_at(theta1)),
            transform(anchor1),
        )
    resolved_radius = base_radius if radius_was_explicit else base_radius * scale
    return path, resolved_radius, resolved_angle


def _fallback_arc_vmobject_init(
    self: _compat.VMobject,
    path: _base.VectorPath,
    kwargs: dict[str, Any],
) -> None:
    """Initialize the native fallback without making Arc a public Path subtype."""

    options = dict(kwargs)
    color = options.pop("color", None)
    raw = _shared._ir.Path(path, **_compat._manim_vmobject_kwargs(options))
    _compat.VMobject.__init__(self, raw)
    if color is not None:
        self.set_color(color)


def _arc_snapshot_json(self: _base.Mobject) -> str:
    handle = _shared._handle_for(self)
    if handle is not None:
        return str(handle.snapshotJson())
    return _shared._snapshot_json(self._current_raw())


def _fallback_arc_query(self: _base.Mobject) -> dict[str, float]:
    raw = self._current_raw()
    commands = raw.geometry.get("vector_path", {}).get("commands", [])
    if not commands or not isinstance(commands[0], dict) or "move_to" not in commands[0]:
        raise ValueError("Arc snapshot has no path start")

    def endpoint(command: object) -> _base.Vec2 | None:
        if not isinstance(command, dict):
            return None
        payload = next(iter(command.values()), None)
        if not isinstance(payload, dict) or "to" not in payload:
            return None
        point = payload["to"]
        return _geometry._world_point(
            self,
            _base.Vec2(float(point["x"]), float(point["y"])),
        )

    start = endpoint(commands[0])
    end = next((point for command in reversed(commands) if (point := endpoint(command)) is not None), None)
    if start is None or end is None:
        raise ValueError("Arc snapshot has incomplete path endpoints")

    center = _base.ORIGIN
    if len(commands) > 1 and isinstance(commands[1], dict) and "cubic_to" in commands[1]:
        payload = commands[1]["cubic_to"]
        control1 = payload["control1"]
        control2 = payload["control2"]
        anchor = payload["to"]
        first_handle = _geometry._world_point(
            self, _base.Vec2(float(control1["x"]), float(control1["y"]))
        )
        second_handle = _geometry._world_point(
            self, _base.Vec2(float(control2["x"]), float(control2["y"]))
        )
        second_anchor = _geometry._world_point(
            self, _base.Vec2(float(anchor["x"]), float(anchor["y"]))
        )
        if start == second_anchor:
            center = start
        else:
            first_tangent = first_handle - start
            second_tangent = second_handle - second_anchor
            first_normal = _base.Vec2(-first_tangent.y, first_tangent.x)
            second_normal = _base.Vec2(-second_tangent.y, second_tangent.x)
            denominator = first_normal.x * second_normal.y - first_normal.y * second_normal.x
            if abs(denominator) > 1e-12:
                delta = second_anchor - start
                parameter = (
                    delta.x * second_normal.y - delta.y * second_normal.x
                ) / denominator
                center = start + parameter * first_normal

    stop = math.atan2(end.y - center.y, end.x - center.x) % _base.TAU
    return {
        "startX": start.x,
        "startY": start.y,
        "endX": end.x,
        "endY": end.y,
        "centerX": center.x,
        "centerY": center.y,
        "stopAngle": stop,
    }


def _arc_query(self: _base.Mobject) -> object:
    if _query_arc is not None:
        return _query_arc(_arc_snapshot_json(self))
    return _fallback_arc_query(self)


def _query_value(record: object, name: str) -> float:
    if isinstance(record, dict):
        return float(record[name])
    return float(getattr(record, name))


def _arc_center_failed(self: _base.Mobject) -> bool:
    raw = self._current_raw()
    commands = raw.geometry.get("vector_path", {}).get("commands", [])
    return not (
        len(commands) > 1
        and isinstance(commands[0], dict)
        and "move_to" in commands[0]
        and isinstance(commands[1], dict)
        and "cubic_to" in commands[1]
    )


def _attach_arc_spec(
    self: _base.Mobject,
    spec: object,
    kwargs: dict[str, Any],
    arc_center: _base.Vec2,
) -> None:
    snapshot_json = str(getattr(spec, "snapshotJson"))
    if _shared._create_handle is None:
        raise RuntimeError("shared Arc construction requires the semantic mobject handle bridge")
    _shared._attach_shared_handle(self, _shared._create_handle(snapshot_json))
    self.radius = float(getattr(spec, "radius"))
    self.start_angle = float(getattr(spec, "startAngle"))
    self.angle = float(getattr(spec, "angle"))
    self.num_components = int(getattr(spec, "numComponents"))
    self.arc_center = arc_center
    self._failed_to_get_center = False

    options = dict(kwargs)
    color = options.pop("color", None)
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


class Arc(_compat.VMobject):
    """ManimCE-compatible Arc whose browser geometry and queries are Rust-owned."""

    def __init__(
        self,
        radius: float | None = 1.0,
        start_angle: float = 0.0,
        angle: float = _base.TAU / 4.0,
        num_components: int = 9,
        arc_center: object = _base.ORIGIN,
        **kwargs: Any,
    ) -> None:
        radius_value = 1.0 if radius is None else _arc_number("radius", radius)
        start_angle_value = _arc_number("start_angle", start_angle)
        angle_value = _arc_number("angle", angle)
        component_count = _arc_num_components(num_components)
        center = _compat._as_vec2(arc_center)

        if _create_arc_spec is not None:
            spec = _create_arc_spec(
                radius_value,
                start_angle_value,
                angle_value,
                component_count,
                center.x,
                center.y,
            )
            _attach_arc_spec(self, spec, kwargs, center)
            return

        path = _fallback_arc_path(
            radius_value,
            start_angle_value,
            angle_value,
            component_count,
            center,
        )
        _fallback_arc_vmobject_init(self, path, kwargs)
        self.radius = radius_value
        self.start_angle = start_angle_value
        self.angle = angle_value
        self.num_components = component_count
        self.arc_center = center
        self._failed_to_get_center = False

    def get_start(self) -> _base.Vec2:
        record = _arc_query(self)
        return _base.Vec2(_query_value(record, "startX"), _query_value(record, "startY"))

    def get_end(self) -> _base.Vec2:
        record = _arc_query(self)
        return _base.Vec2(_query_value(record, "endX"), _query_value(record, "endY"))

    def get_arc_center(self, warning: bool = True) -> _base.Vec2:
        record = _arc_query(self)
        failed = _arc_center_failed(self)
        self._failed_to_get_center = failed
        if failed and warning:
            warnings.warn("Can't find Arc center, using ORIGIN instead", stacklevel=1)
        return _base.Vec2(_query_value(record, "centerX"), _query_value(record, "centerY"))

    def move_arc_center_to(self, point: object) -> Arc:
        target = _compat._as_vec2(point)
        return self.shift(target - self.get_arc_center(warning=False))

    def stop_angle(self) -> float:
        return _query_value(_arc_query(self), "stopAngle")


class ArcBetweenPoints(Arc):
    """ManimCE-compatible endpoint Arc using the shared Rust constructor."""

    def __init__(
        self,
        start: object,
        end: object,
        angle: float = _base.TAU / 4.0,
        radius: float | None = None,
        **kwargs: Any,
    ) -> None:
        options = dict(kwargs)
        component_count = _arc_num_components(options.pop("num_components", 9))
        start_point = _compat._as_vec2(start)
        end_point = _compat._as_vec2(end)
        angle_value = _arc_number("angle", angle)
        radius_value = None if radius is None else _arc_number("radius", radius)

        if _create_arc_between_points_spec is not None:
            spec = _create_arc_between_points_spec(
                start_point.x,
                start_point.y,
                end_point.x,
                end_point.y,
                angle_value,
                radius_value,
                component_count,
            )
            _attach_arc_spec(self, spec, options, _base.ORIGIN)
            self._failed_to_get_center = self.angle == 0.0
            return

        path, resolved_radius, resolved_angle = _fallback_arc_between_points(
            start_point,
            end_point,
            angle_value,
            radius_value,
            component_count,
        )
        _fallback_arc_vmobject_init(self, path, options)
        self.radius = resolved_radius
        self.start_angle = 0.0
        self.angle = resolved_angle
        self.num_components = component_count
        self.arc_center = _base.ORIGIN
        self._failed_to_get_center = resolved_angle == 0.0


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    if _create_dot_handle is not None:
        _geometry.Dot.__init__ = _dot_init
    if _create_triangle_handle is not None:
        _geometry.Triangle.__init__ = _triangle_init

    public = {
        "Arc": Arc,
        "ArcBetweenPoints": ArcBetweenPoints,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)


install()
