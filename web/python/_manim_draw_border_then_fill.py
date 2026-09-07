"""Thin Manim syntax for Rust-owned ``DrawBorderThenFill`` semantics."""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b


_INSTALLED = False


def _double_smooth(t: float) -> float:
    """Pinned ManimCE v0.21 callable used only as an authoring sentinel."""
    value = float(t)
    if value < 0.5:
        return 0.5 * _compat.smooth(2.0 * value)
    return 0.5 * (1.0 + _compat.smooth(2.0 * value - 1.0))


_double_smooth.__name__ = "double_smooth"


def _is_double_smooth(value: object) -> bool:
    return value is _double_smooth or getattr(value, "__name__", None) == "double_smooth"


class DrawBorderThenFill:
    """Author one leaf outline/fill animation in the shared Rust scheduler."""

    def __init__(self, vmobject: object, run_time: float = 2.0,
                 rate_func: object = _double_smooth, stroke_width: float = 2.0,
                 stroke_color: object | None = None, introducer: bool = True,
                 **kwargs: Any) -> None:
        if not isinstance(vmobject, _compat.VMobject) or isinstance(vmobject, _compat.Group):
            raise TypeError("DrawBorderThenFill only works for one leaf VMobject")
        if not _is_double_smooth(rate_func):
            raise NotImplementedError(
                "DrawBorderThenFill currently supports Manim's default double_smooth rate_func only"
            )
        width = float(stroke_width)
        if not math.isfinite(width) or width < 0.0:
            raise ValueError("stroke_width must be finite and non-negative")
        self.mobject = vmobject
        self.target = vmobject
        self.stroke_width = width
        self.stroke_color = None if stroke_color is None else _phase_b._as_color(
            "stroke_color", stroke_color
        )
        self.introducer = bool(introducer)
        self.anim_args = dict(kwargs)
        self.anim_args["run_time"] = float(run_time)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    setattr(_base, "DrawBorderThenFill", DrawBorderThenFill)
    if "DrawBorderThenFill" not in _base.__all__:
        _base.__all__.append("DrawBorderThenFill")


install()
