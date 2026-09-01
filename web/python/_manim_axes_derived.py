"""Derived ManimCE v0.21 CoordinateSystem helpers over retained Axes primitives.

This module owns authoring-only compositions that do not justify new Rust planners or
renderer geometry. Static results remain ordinary retained primitives.
"""

from __future__ import annotations

import json
from typing import Any

import noon as _base
import _manim_axes as _axes
import _manim_compat as _compat
import _manim_semantic_handles as _shared

_INSTALLED = False
_DEFAULT_DX_LINE_COLOR = getattr(
    _base, "PURE_YELLOW", _base.color_from_hex("#FFFF00")
)


def _current_stroke_color(mobject: object, helper_name: str) -> _base.Color:
    """Read the current retained stroke color, including authoring-time recolors."""

    handle = _shared._handle_for(mobject)
    if handle is None or not hasattr(handle, "snapshotJson"):
        raise NotImplementedError(f"{helper_name} requires current retained graph geometry")
    snapshot = json.loads(str(handle.snapshotJson()))
    stroke = snapshot.get("style", {}).get("stroke")
    if not isinstance(stroke, dict):
        raise NotImplementedError(f"{helper_name} requires a retained graph stroke color")
    try:
        return _base.Color(
            float(stroke["red"]),
            float(stroke["green"]),
            float(stroke["blue"]),
            float(stroke["alpha"]),
        )
    except (KeyError, TypeError, ValueError) as error:
        raise TypeError("retained graph stroke color is malformed") from error


def _get_secant_slope_group(
    self: _axes.Axes,
    x: float,
    graph: _axes.ParametricFunction,
    dx: float | None = None,
    dx_line_color: object = _DEFAULT_DX_LINE_COLOR,
    dy_line_color: object | None = None,
    dx_label: object | None = None,
    dy_label: object | None = None,
    include_secant_line: bool = True,
    secant_line_color: object = _base.GREEN,
    secant_line_length: float = 10.0,
) -> _compat.VGroup:
    """Pinned v0.21 secant geometry using retained graph queries and Line leaves."""

    if dx_label is not None or dy_label is not None:
        raise NotImplementedError(
            "get_secant_slope_group labels require retained MathTex/number labels"
        )

    resolved_dx = float(dx or (float(self.x_range[1]) - float(self.x_range[0])) / 10.0)
    p1 = _compat._as_vec2(self.input_to_graph_point(float(x), graph))
    p2 = _compat._as_vec2(self.input_to_graph_point(float(x) + resolved_dx, graph))
    interim_point = _base.Vec2(float(p2.x), float(p1.y))

    group = _compat.VGroup()
    group.dx_line = _compat.Line(
        p1,
        interim_point,
        color=_axes._color("dx_line_color", dx_line_color),
    )
    group.df_line = _compat.Line(
        interim_point,
        p2,
        color=(
            _current_stroke_color(graph, "get_secant_slope_group")
            if dy_line_color is None
            else _axes._color("dy_line_color", dy_line_color)
        ),
    )
    group.add(group.dx_line, group.df_line)

    if include_secant_line:
        group.secant_line = _compat.Line(
            p1,
            p2,
            color=_axes._color("secant_line_color", secant_line_color),
        )
        graph_delta = p2 - p1
        group.secant_line.scale(float(secant_line_length) / graph_delta.length())
        group.add(group.secant_line)

    return group


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _axes.Axes.get_secant_slope_group = _get_secant_slope_group
    _INSTALLED = True
