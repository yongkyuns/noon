"""Inert Manim appearance-lifecycle descriptors.

Canonical Scene.play dispatches these leaves to the shared Rust affine lifecycle
operation. This module owns only Manim argument shape and validation; it does
not create snapshots, tracks, interpolation, or scheduling state.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b


def _point(value: object) -> _base.Vec2:
    if isinstance(value, (_base.Mobject, _compat.Group)):
        return value.get_center()
    return _compat._as_vec2(value)


class GrowFromPoint:
    """Introduce one leaf mobject from an exact scene point."""

    def __init__(self, mobject: object, point: object, point_color: object | None = None, **kwargs: Any) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError("GrowFromPoint currently supports one leaf 2D Mobject; retained groups remain partial")
        self.mobject = mobject
        self.target = mobject
        self.point = _point(point)
        self.point_color = None if point_color is None else _phase_b._as_color("point_color", point_color)
        self.anim_args = dict(kwargs)


class GrowFromCenter(GrowFromPoint):
    """Introduce one leaf mobject from its shared Rust center query."""

    def __init__(self, mobject: object, point_color: object | None = None, **kwargs: Any) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError("GrowFromCenter currently supports one leaf 2D Mobject; retained groups remain partial")
        super().__init__(mobject, mobject.get_center(), point_color=point_color, **kwargs)


class GrowFromEdge(GrowFromPoint):
    """Introduce one leaf mobject from its shared Rust critical-point query."""

    def __init__(self, mobject: object, edge: object, point_color: object | None = None, **kwargs: Any) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError("GrowFromEdge currently supports one leaf 2D Mobject; retained groups remain partial")
        direction = _compat._as_vec2(edge)
        super().__init__(mobject, mobject.get_critical_point(direction), point_color=point_color, **kwargs)
        self.edge = direction


class SpinInFromNothing(GrowFromCenter):
    """Grow one centered leaf with a shared Rust rotation channel."""

    def __init__(self, mobject: object, angle: float = math.pi / 2.0, point_color: object | None = None, **kwargs: Any) -> None:
        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("SpinInFromNothing angle must be finite")
        self.angle = value
        super().__init__(mobject, point_color=point_color, **kwargs)


def install() -> None:
    public = {
        "GrowFromPoint": GrowFromPoint,
        "GrowFromCenter": GrowFromCenter,
        "GrowFromEdge": GrowFromEdge,
        "SpinInFromNothing": SpinInFromNothing,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)


install()
