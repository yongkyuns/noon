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


def _set_x(self: _base.Mobject, x: float) -> _base.Mobject:
    """Set the center x coordinate through Rust-owned ``move_to`` semantics."""

    value = _shared._ir._finite_number("x", x)
    return _shared._move_to(
        self,
        _base.Vec2(value, 0.0),
        coor_mask=(1.0, 0.0, 0.0),
    )


def _set_y(self: _base.Mobject, y: float) -> _base.Mobject:
    """Set the center y coordinate through Rust-owned ``move_to`` semantics."""

    value = _shared._ir._finite_number("y", y)
    return _shared._move_to(
        self,
        _base.Vec2(0.0, value),
        coor_mask=(0.0, 1.0, 0.0),
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
    _base.Mobject.set_x = _set_x
    _base.Mobject.set_y = _set_y
    if _create_dot_handle is not None:
        _geometry.Dot.__init__ = _dot_init
    if _create_triangle_handle is not None:
        _geometry.Triangle.__init__ = _triangle_init


install()
