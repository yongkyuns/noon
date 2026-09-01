"""ManimCE v0.21 ComplexPlane scalar facade over retained NumberPlane semantics."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_number_plane as _number_plane

_INSTALLED = False


class ComplexPlane(_number_plane.NumberPlane):
    """Retained NumberPlane specialized for scalar complex-number conversion."""

    def __init__(self, **kwargs: Any) -> None:
        super().__init__(**kwargs)

    def number_to_point(self, number: float | complex) -> _base.Vec2:
        value = complex(number)
        return self.coords_to_point(value.real, value.imag)

    def n2p(self, number: float | complex) -> _base.Vec2:
        return self.number_to_point(number)

    def point_to_number(self, point: object) -> complex:
        x, y = self.point_to_coords(point)
        return complex(x, y)

    def p2n(self, point: object) -> complex:
        return self.point_to_number(point)

    def get_coordinate_labels(self, *numbers: object, **kwargs: Any):
        del numbers, kwargs
        raise NotImplementedError(
            "ComplexPlane coordinate labels require retained number/MathTex labels"
        )

    def add_coordinates(self, *numbers: object, **kwargs: Any):
        del numbers, kwargs
        raise NotImplementedError(
            "ComplexPlane.add_coordinates requires retained number/MathTex labels"
        )


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    setattr(_base, "ComplexPlane", ComplexPlane)
    setattr(_compat, "ComplexPlane", ComplexPlane)
    if "ComplexPlane" not in _base.__all__:
        _base.__all__.append("ComplexPlane")
    _INSTALLED = True
