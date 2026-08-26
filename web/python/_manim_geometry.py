"""ManimCE v0.21 geometry breadth over Noon's qualified analytic primitives.

This module intentionally reuses the Circle semantic/render representation for shapes
whose observable Manim behavior is an affine circle specialization. It adds no
renderer-only geometry path and keeps source compatibility in the thin Python facade.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat

DEFAULT_DOT_RADIUS = 0.08


class Dot(_compat.Circle):
    """Manim-compatible small filled circle."""

    def __init__(
        self,
        point: object = _base.ORIGIN,
        radius: float = DEFAULT_DOT_RADIUS,
        stroke_width: float = 0.0,
        fill_opacity: float = 1.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        super().__init__(
            radius=radius,
            stroke_width=stroke_width,
            fill_opacity=fill_opacity,
            color=color,
            **kwargs,
        )
        self.move_to(_compat._as_vec2(point))


class Ellipse(_compat.Circle):
    """Manim-compatible affine circle with independent width and height.

    Noon keeps the renderer geometry analytic. ManimCE's observable VMobject layout,
    however, measures the point/control-point array of its eight cubic circle segments.
    For a rotated non-uniform ellipse that control hull is slightly larger than the
    true analytic extrema, so layout queries intentionally reproduce Manim's hull.
    """

    _CUBIC_HANDLE_FACTOR = 4.0 / 3.0 * math.tan(math.pi / 16.0)

    def __init__(self, width: float = 2.0, height: float = 1.0, **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self.stretch_to_fit_width(float(width))
        self.stretch_to_fit_height(float(height))

    def _manim_layout_bounds(self) -> tuple[_base.Vec2, _base.Vec2]:
        raw = self._current_raw()
        radius = float(raw.geometry["circle"]["radius"])
        transform = raw.transform
        scale_x = float(transform["scale"]["x"])
        scale_y = float(transform["scale"]["y"])
        rotation = float(transform["rotation"])
        translation_x = float(transform["translation"]["x"])
        translation_y = float(transform["translation"]["y"])
        sine = math.sin(rotation)
        cosine = math.cos(rotation)
        factor = self._CUBIC_HANDLE_FACTOR

        points: list[_base.Vec2] = []
        for index in range(8):
            start_angle = index * math.pi / 4.0
            end_angle = (index + 1) * math.pi / 4.0
            start = _base.Vec2(math.cos(start_angle), math.sin(start_angle))
            end = _base.Vec2(math.cos(end_angle), math.sin(end_angle))
            start_tangent = _base.Vec2(-math.sin(start_angle), math.cos(start_angle))
            end_tangent = _base.Vec2(-math.sin(end_angle), math.cos(end_angle))
            control1 = start + factor * start_tangent
            control2 = end - factor * end_tangent

            for point in (start, control1, control2, end):
                x = radius * point.x * scale_x
                y = radius * point.y * scale_y
                points.append(
                    _base.Vec2(
                        x * cosine - y * sine + translation_x,
                        x * sine + y * cosine + translation_y,
                    )
                )

        return (
            _base.Vec2(
                min(point.x for point in points),
                min(point.y for point in points),
            ),
            _base.Vec2(
                max(point.x for point in points),
                max(point.y for point in points),
            ),
        )

    @property
    def width(self) -> float:
        minimum, maximum = self._manim_layout_bounds()
        return maximum.x - minimum.x

    @width.setter
    def width(self, value: float) -> None:
        self.scale_to_fit_width(float(value))

    @property
    def height(self) -> float:
        minimum, maximum = self._manim_layout_bounds()
        return maximum.y - minimum.y

    @height.setter
    def height(self, value: float) -> None:
        self.scale_to_fit_height(float(value))


def _bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Use wrapper-specific Manim layout bounds while preserving flat runtime data."""

    leaves = _compat._leaf_mobjects(value)
    bounds: list[tuple[_base.Vec2, _base.Vec2]] = []
    for member in leaves:
        custom = getattr(member, "_manim_layout_bounds", None)
        bound = custom() if callable(custom) else _base._bounds(member._current_raw())
        if bound is not None:
            bounds.append(bound)
    if not bounds:
        return None
    return (
        _base.Vec2(
            min(bound[0].x for bound in bounds),
            min(bound[0].y for bound in bounds),
        ),
        _base.Vec2(
            max(bound[1].x for bound in bounds),
            max(bound[1].y for bound in bounds),
        ),
    )


def install() -> None:
    public = {
        "DEFAULT_DOT_RADIUS": DEFAULT_DOT_RADIUS,
        "Dot": Dot,
        "Ellipse": Ellipse,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        if name != "DEFAULT_DOT_RADIUS":
            setattr(_compat, name, value)

    # Existing compatibility layout methods resolve this module global at call time,
    # so the hook affects only Manim-facing authoring/layout and never renderer bounds.
    _compat._bounds_for = _bounds_for

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports


install()
