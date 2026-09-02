"""Thin Manim DashedLine adapter backed by shared Rust geometry."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared

try:
    from js import noonCreateAuthoringDashedLineHandle as _create_dashed_line_handle
except ImportError:
    _create_dashed_line_handle = None

_INSTALLED = False


class DashedLine(_compat.Line):
    """Straight Manim DashedLine whose dash geometry is authored in shared Rust."""

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        dash_length: float = 0.05,
        dashed_ratio: float = 0.5,
        **kwargs: Any,
    ) -> None:
        if _create_dashed_line_handle is None:
            raise RuntimeError("DashedLine requires the shared browser geometry bridge")

        start_value = _compat._as_vec2(start)
        end_value = _compat._as_vec2(end)
        dash_length_value = _shared._ir._positive_number("dash_length", dash_length)
        dashed_ratio_value = _shared._ir._finite_number("dashed_ratio", dashed_ratio)
        if not 0.0 <= dashed_ratio_value <= 1.0:
            raise ValueError("dashed_ratio must be within [0, 1]")

        options = dict(kwargs)
        color = options.pop("color", None)
        _shared._attach_shared_handle(
            self,
            _create_dashed_line_handle(
                start_value.x,
                start_value.y,
                end_value.x,
                end_value.y,
                dash_length_value,
                dashed_ratio_value,
            ),
        )
        self.dash_length = dash_length_value
        self.dashed_ratio = dashed_ratio_value
        _shared._apply_shared_constructor_kwargs(self, options)
        if color is not None:
            parsed = _shared._phase_b._as_color("color", color)
            _shared._set_color(self, parsed)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    _base.DashedLine = DashedLine
    _compat.DashedLine = DashedLine
    if "DashedLine" not in _base.__all__:
        _base.__all__.append("DashedLine")


install()
