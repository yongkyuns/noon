"""Thin Manim Brace adapters backed by shared Rust retained geometry."""

from __future__ import annotations

from typing import Any

from _noon_errors import engine_call

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared


def _require_geometry_host(name: str) -> None:
    if _shared._create_geometry_handle is None:
        raise RuntimeError(f"{name} requires the shared Rust authoring host")


def _brace_target_handle(mobject: object):
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("Brace target must be a Mobject")
    if isinstance(mobject, _compat.Group):
        handle = getattr(mobject, "_semantic_family_handle", None)
    else:
        handle = _shared._handle_for(mobject)
    if handle is None or not hasattr(handle, "beginBrace"):
        raise NotImplementedError(
            "Brace target requires a current shared semantic object/family handle"
        )
    return handle


def _brace_constructor_options(
    options: dict[str, Any],
    *,
    stroke_width: float,
    fill_opacity: float,
    background_stroke_width: float,
) -> dict[str, Any]:
    background_width = _shared._ir._finite_number(
        "background_stroke_width", background_stroke_width
    )
    if background_width != 0.0:
        raise NotImplementedError(
            "Brace background stroke is not represented by Noon's ordinary retained path style"
        )
    # background_stroke_color is visually inert while background_stroke_width == 0.
    options.pop("background_stroke_color", None)
    options["stroke_width"] = stroke_width
    options["fill_opacity"] = fill_opacity
    return options


def _finish_candidate(
    owner: _base.Mobject,
    candidate: object,
    name: str,
    options: dict[str, Any],
) -> None:
    color = options.pop("color", None)
    _shared._apply_shared_constructor_options(candidate, options)
    if color is not None:
        parsed = _shared._compat._as_color("color", color)
        _shared._apply_constructor_color(candidate, parsed)
    _shared._attach_geometry_options(owner, candidate, name)


class Brace(_compat.VMobject):
    """ManimCE v0.21 Brace whose path/layout policy is owned by shared Rust."""

    def __init__(
        self,
        mobject: _base.Mobject,
        direction: object = _base.DOWN,
        buff: float = 0.2,
        sharpness: float = 2.0,
        stroke_width: float = 0.0,
        fill_opacity: float = 1.0,
        background_stroke_width: float = 0.0,
        background_stroke_color: object = _base.BLACK,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("Brace")
        target = _brace_target_handle(mobject)
        direction_value = _base._as_vec2(direction)
        buff_value = _shared._ir._finite_number("buff", buff)
        sharpness_value = _shared._ir._finite_number("sharpness", sharpness)
        stroke_value = _shared._ir._finite_number("stroke_width", stroke_width)
        fill_value = _shared._compat._opacity("fill_opacity", fill_opacity)
        options = dict(kwargs)
        options["background_stroke_color"] = background_stroke_color
        options = _brace_constructor_options(
            options,
            stroke_width=stroke_value,
            fill_opacity=fill_value,
            background_stroke_width=background_stroke_width,
        )
        candidate = engine_call(
            target.beginBrace,
            direction_value.x,
            direction_value.y,
            buff_value,
            sharpness_value,
            operation="Brace",
        )
        _finish_candidate(self, candidate, "Brace", options)
        self.buff = buff_value
        self.sharpness = sharpness_value
        self.direction = direction_value


class BraceBetweenPoints(Brace):
    """Brace between two points without allocating a temporary semantic Line."""

    def __init__(
        self,
        point_1: object,
        point_2: object,
        direction: object = _base.ORIGIN,
        **kwargs: Any,
    ) -> None:
        _require_geometry_host("BraceBetweenPoints")
        first = _base._as_vec2(point_1)
        second = _base._as_vec2(point_2)
        direction_value = _base._as_vec2(direction)
        options = dict(kwargs)
        buff_value = _shared._ir._finite_number("buff", options.pop("buff", 0.2))
        sharpness_value = _shared._ir._finite_number(
            "sharpness", options.pop("sharpness", 2.0)
        )
        stroke_value = _shared._ir._finite_number(
            "stroke_width", options.pop("stroke_width", 0.0)
        )
        fill_value = _shared._compat._opacity(
            "fill_opacity", options.pop("fill_opacity", 1.0)
        )
        background_width = options.pop("background_stroke_width", 0.0)
        options = _brace_constructor_options(
            options,
            stroke_width=stroke_value,
            fill_opacity=fill_value,
            background_stroke_width=background_width,
        )
        candidate = engine_call(
            _shared._geometry_options.braceBetweenPoints,
            first.x,
            first.y,
            second.x,
            second.y,
            direction_value.x,
            direction_value.y,
            buff_value,
            sharpness_value,
            operation="BraceBetweenPoints",
        )
        _finish_candidate(self, candidate, "BraceBetweenPoints", options)
        self.buff = buff_value
        self.sharpness = sharpness_value
        self.direction = direction_value
