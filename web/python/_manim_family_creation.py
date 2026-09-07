"""Inert Manim-compatible Write/Unwrite syntax for shared Rust playback."""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b
import _manim_rate_functions as _rate_functions
import _manim_typst as _typst


_INSTALLED = False


def _native_text(value: object) -> bool:
    return isinstance(value, _typst.Text)


class Write:
    """Simulate writing retained native Text with ManimCE v0.21 family semantics."""

    def __init__(
        self,
        vmobject: object,
        rate_func: object = _rate_functions.linear,
        reverse: bool = False,
        **kwargs: Any,
    ) -> None:
        if not isinstance(vmobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Write target must be a Mobject or Group")

        animation_kwargs = dict(kwargs)
        if "introducer" in animation_kwargs:
            raise TypeError("Write owns introducer from its reverse option")
        stroke_width = float(animation_kwargs.pop("stroke_width", 2.0))
        stroke_color = animation_kwargs.pop("stroke_color", None)
        if not math.isfinite(stroke_width) or stroke_width < 0.0:
            raise ValueError("Write stroke_width must be finite and non-negative")
        leaves = _compat._leaf_mobjects(vmobject)
        retained_text = any(_native_text(member) for member in leaves)
        if retained_text and (
            not math.isclose(stroke_width, 2.0, abs_tol=1e-15) or stroke_color is not None
        ):
            raise NotImplementedError(
                "retained Text Write currently supports Manim's default outline style only"
            )

        remover = animation_kwargs.pop("remover", bool(reverse))
        self.mobject = vmobject
        self.target = vmobject
        self.reverse = bool(reverse)
        self.introducer = not self.reverse
        self.remover = bool(remover)
        self.reverse_rate_function = bool(
            animation_kwargs.get("reverse_rate_function", False)
        )
        self.stroke_width = stroke_width
        self.stroke_color = (
            None
            if stroke_color is None
            else _phase_b._as_color("stroke_color", stroke_color)
        )
        animation_kwargs["rate_func"] = rate_func
        self.anim_args = animation_kwargs


class Unwrite(Write):
    """Simulate erasing retained native Text with ManimCE v0.21 family semantics."""

    def __init__(
        self,
        vmobject: object,
        rate_func: object = _rate_functions.linear,
        reverse: bool = True,
        **kwargs: Any,
    ) -> None:
        if "reverse_rate_function" in kwargs:
            raise TypeError("Unwrite owns reverse_rate_function=True")
        animation_kwargs = dict(kwargs)
        animation_kwargs["reverse_rate_function"] = True
        super().__init__(
            vmobject,
            rate_func=rate_func,
            reverse=reverse,
            **animation_kwargs,
        )



def install() -> None:
    """Install inert Write/Unwrite syntax for shared canonical playback."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    for name, value in {"Write": Write, "Unwrite": Unwrite}.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
