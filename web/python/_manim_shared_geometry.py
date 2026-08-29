"""Thin Manim geometry constructor adapters backed by shared Rust semantics.

This module intentionally patches only constructors whose observable geometry/layout
contract is already owned by Rust. Class identity and inheritance stay Python-facing,
while path construction, winding, defaults, transforms and vertex queries stay shared.
"""

from __future__ import annotations

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
    from js import noonCreateAuthoringPolygonHandle as _create_polygon_handle
except ImportError:
    _create_polygon_handle = None
try:
    from js import noonCreateAuthoringPolygramHandle as _create_polygram_handle
except ImportError:
    _create_polygram_handle = None
try:
    from js import noonCreateAuthoringRegularPolygonHandle as _create_regular_polygon_handle
except ImportError:
    _create_regular_polygon_handle = None
try:
    from js import noonCreateAuthoringRegularPolygramHandle as _create_regular_polygram_handle
except ImportError:
    _create_regular_polygram_handle = None
try:
    from js import noonCreateAuthoringStarHandle as _create_star_handle
except ImportError:
    _create_star_handle = None

_ORIGINAL_DOT_INIT = _geometry.Dot.__init__
_ORIGINAL_TRIANGLE_INIT = _geometry.Triangle.__init__
_INSTALLED = False


def _set_shared_color(self: _base.Mobject, color: object) -> None:
    """Apply Manim ``color`` through the shared semantic handle."""

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
    """Compatibility path for bridges that expose Triangle but not the full family."""

    if _create_triangle_handle is None:
        _ORIGINAL_TRIANGLE_INIT(self, **kwargs)
        return

    options = dict(kwargs)
    color = options.pop("color", None)
    _shared._attach_shared_handle(self, _create_triangle_handle())
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


def _vertex2(value: object) -> _base.Vec2:
    return _compat._as_vec2(value)


def _flatten_vertices(vertices: tuple[object, ...]) -> list[float]:
    flat: list[float] = []
    for vertex in vertices:
        point = _vertex2(vertex)
        flat.extend((float(point.x), float(point.y)))
    return flat


def _flatten_vertex_groups(vertex_groups: tuple[object, ...]) -> tuple[list[float], list[int]]:
    flat: list[float] = []
    lengths: list[int] = []
    for group in vertex_groups:
        try:
            vertices = tuple(group)  # type: ignore[arg-type]
        except TypeError as error:
            raise TypeError("Polygram vertex groups must be iterable") from error
        lengths.append(len(vertices))
        flat.extend(_flatten_vertices(vertices))
    return flat, lengths


def _constructor_options(kwargs: dict[str, Any]) -> tuple[object | None, dict[str, Any]]:
    options = dict(kwargs)
    color = options.pop("color", None)
    return color, options


def _attach_constructed(
    self: _base.Mobject,
    handle: object,
    color: object | None,
    options: dict[str, Any],
) -> None:
    _shared._attach_shared_handle(self, handle)
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


def _decode_vertex_groups(self: _base.Mobject) -> list[list[tuple[float, float, float]]]:
    handle = _shared._handle_for(self)
    if handle is None or not hasattr(handle, "manimVertexGroups"):
        raise RuntimeError("polygram vertex queries require a current shared semantic handle")
    encoded = handle.manimVertexGroups()
    try:
        coordinates = [float(value) for value in encoded.coordinates()]
        lengths = [int(value) for value in encoded.groupLengths()]
    finally:
        encoded.free()

    groups: list[list[tuple[float, float, float]]] = []
    cursor = 0
    for length in lengths:
        group: list[tuple[float, float, float]] = []
        for _ in range(length):
            group.append((coordinates[cursor], coordinates[cursor + 1], 0.0))
            cursor += 2
        groups.append(group)
    if cursor != len(coordinates):
        raise RuntimeError("shared polygram vertex query returned inconsistent group lengths")
    return groups


class Polygram(_compat.VMobject):
    def __init__(
        self,
        *vertex_groups: object,
        color: _base.Color = _base.BLUE,
        **kwargs: Any,
    ) -> None:
        if _create_polygram_handle is None:
            raise RuntimeError("shared Polygram authoring bridge is unavailable")
        coordinates, lengths = _flatten_vertex_groups(vertex_groups)
        _attach_constructed(
            self,
            _create_polygram_handle(coordinates, lengths),
            color,
            dict(kwargs),
        )

    def get_vertex_groups(self) -> list[list[tuple[float, float, float]]]:
        return _decode_vertex_groups(self)

    def get_vertices(self) -> list[tuple[float, float, float]]:
        return [vertex for group in self.get_vertex_groups() for vertex in group]


class Polygon(Polygram):
    def __init__(self, *vertices: object, **kwargs: Any) -> None:
        if _create_polygon_handle is None:
            raise RuntimeError("shared Polygon authoring bridge is unavailable")
        color, options = _constructor_options(kwargs)
        _attach_constructed(
            self,
            _create_polygon_handle(_flatten_vertices(vertices)),
            color,
            options,
        )


class RegularPolygram(Polygram):
    def __init__(
        self,
        num_vertices: int,
        *,
        density: int = 2,
        radius: float = 1.0,
        start_angle: float | None = None,
        **kwargs: Any,
    ) -> None:
        if _create_regular_polygram_handle is None:
            raise RuntimeError("shared RegularPolygram authoring bridge is unavailable")
        color, options = _constructor_options(kwargs)
        _attach_constructed(
            self,
            _create_regular_polygram_handle(
                int(num_vertices),
                int(density),
                float(radius),
                start_angle,
            ),
            color,
            options,
        )
        self.num_vertices = int(num_vertices)
        self.density = int(density)
        self.radius = float(radius)
        self.start_angle = start_angle


class RegularPolygon(RegularPolygram):
    def __init__(self, n: int = 6, **kwargs: Any) -> None:
        if _create_regular_polygon_handle is None:
            raise RuntimeError("shared RegularPolygon authoring bridge is unavailable")
        options = dict(kwargs)
        radius = float(options.pop("radius", 1.0))
        start_angle = options.pop("start_angle", None)
        color, options = _constructor_options(options)
        _attach_constructed(
            self,
            _create_regular_polygon_handle(int(n), radius, start_angle),
            color,
            options,
        )
        self.num_vertices = int(n)
        self.density = 1
        self.radius = radius
        self.start_angle = start_angle


class Triangle(RegularPolygon):
    def __init__(self, **kwargs: Any) -> None:
        if _create_triangle_handle is None:
            raise RuntimeError("shared Triangle authoring bridge is unavailable")
        color, options = _constructor_options(kwargs)
        _attach_constructed(self, _create_triangle_handle(), color, options)
        self.num_vertices = 3
        self.density = 1
        self.radius = 1.0
        self.start_angle = None


class Star(Polygon):
    def __init__(
        self,
        n: int = 5,
        *,
        outer_radius: float = 1.0,
        inner_radius: float | None = None,
        density: int = 2,
        start_angle: float = _base.PI / 2,
        **kwargs: Any,
    ) -> None:
        if _create_star_handle is None:
            raise RuntimeError("shared Star authoring bridge is unavailable")
        color, options = _constructor_options(kwargs)
        _attach_constructed(
            self,
            _create_star_handle(
                int(n),
                float(outer_radius),
                inner_radius,
                int(density),
                float(start_angle),
            ),
            color,
            options,
        )
        self.num_vertices = int(n)
        self.outer_radius = float(outer_radius)
        self.inner_radius = inner_radius
        self.density = int(density)
        self.start_angle = float(start_angle)


def _polygram_bridge_available() -> bool:
    return all(
        value is not None
        for value in (
            _create_polygon_handle,
            _create_polygram_handle,
            _create_regular_polygon_handle,
            _create_regular_polygram_handle,
            _create_triangle_handle,
            _create_star_handle,
        )
    )


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    if _create_dot_handle is not None:
        _geometry.Dot.__init__ = _dot_init

    if _polygram_bridge_available():
        public = {
            "Polygram": Polygram,
            "Polygon": Polygon,
            "RegularPolygram": RegularPolygram,
            "RegularPolygon": RegularPolygon,
            "Triangle": Triangle,
            "Star": Star,
        }
        for name, value in public.items():
            setattr(_base, name, value)
            setattr(_compat, name, value)
            setattr(_geometry, name, value)
        exports = list(_base.__all__)
        for name in public:
            if name not in exports:
                exports.append(name)
        _base.__all__ = exports
    elif _create_triangle_handle is not None:
        _geometry.Triangle.__init__ = _triangle_init


install()
