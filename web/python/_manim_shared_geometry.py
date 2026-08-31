"""Thin Manim geometry adapters backed by shared Rust semantics.

This module patches only operations whose full observable geometry/layout contract is
already owned by Rust. Class identity and inheritance remain unchanged where an
established compatibility class already exists.
"""

from __future__ import annotations

import operator
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
    from js import noonCreateAuthoringRoundedRectangleHandle as _create_rounded_rectangle_handle
except ImportError:
    _create_rounded_rectangle_handle = None

try:
    from js import noonCreateAuthoringAnnularSectorHandle as _create_annular_sector_handle
except ImportError:
    _create_annular_sector_handle = None

try:
    from js import noonCreateAuthoringSectorHandle as _create_sector_handle
except ImportError:
    _create_sector_handle = None

try:
    from js import noonCreateAuthoringAnnulusHandle as _create_annulus_handle
except ImportError:
    _create_annulus_handle = None

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


def _coordinate_mask(dim: int) -> tuple[float, float, float]:
    if isinstance(dim, bool) or not isinstance(dim, int):
        raise TypeError("dim must be an integer")
    if dim == 0:
        return (1.0, 0.0, 0.0)
    if dim == 1:
        return (0.0, 1.0, 0.0)
    raise NotImplementedError("Noon's shared 2D coordinate placement supports only x/y")


def _set_coord(
    self: _base.Mobject,
    value: float,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional coordinate through shared critical-point placement."""

    coordinate = _shared._ir._finite_number("value", value)
    mask = _coordinate_mask(dim)
    point = _base.Vec2(coordinate if dim == 0 else 0.0, coordinate if dim == 1 else 0.0)
    return _shared._move_to(
        self,
        point,
        aligned_edge=direction,
        coor_mask=mask,
    )


def _set_x(
    self: _base.Mobject,
    x: float,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional x coordinate through shared ``set_coord`` semantics."""

    return _set_coord(self, x, 0, direction)


def _set_y(
    self: _base.Mobject,
    y: float,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Set a directional y coordinate through shared ``set_coord`` semantics."""

    return _set_coord(self, y, 1, direction)


def _match_coord(
    self: _base.Mobject,
    mobject: _base.Mobject,
    dim: int,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional coordinate through shared critical-point placement."""

    return _shared._move_to(
        self,
        mobject,
        aligned_edge=direction,
        coor_mask=_coordinate_mask(dim),
    )


def _match_x(
    self: _base.Mobject,
    mobject: _base.Mobject,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional x coordinate through shared ``match_coord`` semantics."""

    return _match_coord(self, mobject, 0, direction)


def _match_y(
    self: _base.Mobject,
    mobject: _base.Mobject,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Match a directional y coordinate through shared ``match_coord`` semantics."""

    return _match_coord(self, mobject, 1, direction)


def _rotate_about_origin(
    self: _base.Mobject,
    angle: float,
    axis: object = _compat.OUT,
    **kwargs: Any,
) -> _base.Mobject:
    """Rotate through the shared Rust transform path around Manim's origin."""

    return _shared._rotate(
        self,
        angle,
        axis,
        about_point=_base.ORIGIN,
        **kwargs,
    )


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


class RoundedRectangle(_compat.Rectangle):
    """Scalar-radius Manim RoundedRectangle backed by shared Rust geometry."""

    def __init__(self, corner_radius: float = 0.5, **kwargs: Any) -> None:
        if _create_rounded_rectangle_handle is None:
            raise RuntimeError("RoundedRectangle requires the shared browser geometry bridge")
        if isinstance(corner_radius, (list, tuple)):
            raise NotImplementedError(
                "per-corner RoundedRectangle radii are not exposed by the browser bridge yet"
            )

        options = dict(kwargs)
        width = _shared._ir._positive_number("width", options.pop("width", 4.0))
        height = _shared._ir._positive_number("height", options.pop("height", 2.0))
        radius = _shared._ir._finite_number("corner_radius", corner_radius)
        color = options.pop("color", None)
        _shared._attach_shared_handle(
            self,
            _create_rounded_rectangle_handle(width, height, radius),
        )
        self.width_value = width
        self.height_value = height
        self.corner_radius = radius
        _shared._apply_shared_constructor_kwargs(self, options)
        if color is not None:
            _set_shared_color(self, color)


def _sector_component_count(value: object) -> int:
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


def _sector_options(
    kwargs: dict[str, Any],
) -> tuple[dict[str, Any], int, _base.Vec2]:
    options = dict(kwargs)
    component_count = _sector_component_count(options.pop("num_components", 9))
    center = _compat._as_vec2(options.pop("arc_center", _base.ORIGIN))
    return options, component_count, center


def _finish_sector_style(
    self: _base.Mobject,
    kwargs: dict[str, Any],
    *,
    fill_opacity: float,
    stroke_width: float,
    color: object,
) -> None:
    options = dict(kwargs)
    options["fill_opacity"] = fill_opacity
    options["stroke_width"] = stroke_width
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        _set_shared_color(self, color)


class AnnularSector(_compat.VMobject):
    """Manim-compatible annular sector backed by the shared Rust constructor."""

    def __init__(
        self,
        inner_radius: float = 1.0,
        outer_radius: float = 2.0,
        angle: float = _base.TAU / 4.0,
        start_angle: float = 0.0,
        fill_opacity: float = 1.0,
        stroke_width: float = 0.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        if _create_annular_sector_handle is None:
            raise RuntimeError("AnnularSector requires the shared browser geometry bridge")

        options, component_count, center = _sector_options(kwargs)
        inner = _shared._ir._finite_number("inner_radius", inner_radius)
        outer = _shared._ir._finite_number("outer_radius", outer_radius)
        angle_value = _shared._ir._finite_number("angle", angle)
        start_value = _shared._ir._finite_number("start_angle", start_angle)
        _shared._attach_shared_handle(
            self,
            _create_annular_sector_handle(
                inner,
                outer,
                angle_value,
                start_value,
                component_count,
                center.x,
                center.y,
            ),
        )
        self.inner_radius = inner
        self.outer_radius = outer
        self.angle = angle_value
        self.start_angle = start_value
        self.num_components = component_count
        self.arc_center = center
        _finish_sector_style(
            self,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )


class Sector(AnnularSector):
    """Manim-compatible circle sector backed by the shared Rust constructor."""

    def __init__(self, radius: float = 1.0, **kwargs: Any) -> None:
        if _create_sector_handle is None:
            raise RuntimeError("Sector requires the shared browser geometry bridge")

        options = dict(kwargs)
        fill_opacity = options.pop("fill_opacity", 1.0)
        stroke_width = options.pop("stroke_width", 0.0)
        color = options.pop("color", _base.WHITE)
        start_angle = options.pop("start_angle", 0.0)
        angle = options.pop("angle", _base.TAU / 4.0)
        options, component_count, center = _sector_options(options)
        radius_value = _shared._ir._finite_number("radius", radius)
        angle_value = _shared._ir._finite_number("angle", angle)
        start_value = _shared._ir._finite_number("start_angle", start_angle)
        _shared._attach_shared_handle(
            self,
            _create_sector_handle(
                radius_value,
                angle_value,
                start_value,
                component_count,
                center.x,
                center.y,
            ),
        )
        self.inner_radius = 0.0
        self.outer_radius = radius_value
        self.angle = angle_value
        self.start_angle = start_value
        self.num_components = component_count
        self.arc_center = center
        _finish_sector_style(
            self,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )


class Annulus(_compat.VMobject):
    """Manim-compatible annulus backed by the shared Rust constructor."""

    def __init__(
        self,
        inner_radius: float = 1.0,
        outer_radius: float = 2.0,
        fill_opacity: float = 1.0,
        stroke_width: float = 0.0,
        color: _base.Color = _base.WHITE,
        mark_paths_closed: bool = False,
        **kwargs: Any,
    ) -> None:
        if _create_annulus_handle is None:
            raise RuntimeError("Annulus requires the shared browser geometry bridge")

        options, component_count, center = _sector_options(kwargs)
        inner = _shared._ir._finite_number("inner_radius", inner_radius)
        outer = _shared._ir._finite_number("outer_radius", outer_radius)
        _shared._attach_shared_handle(
            self,
            _create_annulus_handle(
                inner,
                outer,
                component_count,
                center.x,
                center.y,
            ),
        )
        self.inner_radius = inner
        self.outer_radius = outer
        self.mark_paths_closed = bool(mark_paths_closed)
        self.num_components = component_count
        self.arc_center = center
        _finish_sector_style(
            self,
            options,
            fill_opacity=fill_opacity,
            stroke_width=stroke_width,
            color=color,
        )


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    _base.Mobject.set_coord = _set_coord
    _base.Mobject.set_x = _set_x
    _base.Mobject.set_y = _set_y
    _base.Mobject.match_coord = _match_coord
    _base.Mobject.match_x = _match_x
    _base.Mobject.match_y = _match_y
    _base.Mobject.rotate_about_origin = _rotate_about_origin
    if _create_dot_handle is not None:
        _geometry.Dot.__init__ = _dot_init
    if _create_triangle_handle is not None:
        _geometry.Triangle.__init__ = _triangle_init

    public = {
        "RoundedRectangle": RoundedRectangle,
        "AnnularSector": AnnularSector,
        "Sector": Sector,
        "Annulus": Annulus,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)


install()