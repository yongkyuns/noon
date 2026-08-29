"""Thin Manim geometry adapters backed by shared Rust semantics.

This module patches only operations whose full observable geometry/layout contract is
already owned by Rust. Class identity and inheritance remain unchanged.
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


install()
