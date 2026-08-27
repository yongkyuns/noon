"""Thin Manim geometry constructor adapters backed by shared Rust semantics.

This module intentionally patches only constructors whose full observable geometry/layout
contract is already owned by Rust.  Class identity and inheritance remain unchanged.
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
        self.set_color(color)


def _triangle_init(self: _geometry.Triangle, **kwargs: Any) -> None:
    if _create_triangle_handle is None:
        _ORIGINAL_TRIANGLE_INIT(self, **kwargs)
        return

    options = dict(kwargs)
    color = options.pop("color", None)
    _shared._attach_shared_handle(self, _create_triangle_handle())
    _shared._apply_shared_constructor_kwargs(self, options)
    if color is not None:
        self.set_color(color)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    if _create_dot_handle is not None:
        _geometry.Dot.__init__ = _dot_init
    if _create_triangle_handle is not None:
        _geometry.Triangle.__init__ = _triangle_init


install()
