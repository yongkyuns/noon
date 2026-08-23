"""ManimCE-compatible public authoring foundation for the browser Python frontend.

This module deliberately changes only Python authoring semantics. Objects still lower to
Noon's existing semantic snapshots/tracks and analytic/path renderer representations.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import noon as _base

_BaseMobject = _base.Mobject
_BaseScene = _base.Scene
_ir = _base._ir

_INSTALLED = False


def _as_vec2(value: object) -> _base.Vec2:
    """Accept Noon's Vec2 plus common Manim 2D/3D vector inputs.

    Manim commonly represents 2D directions as three-component NumPy vectors. Noon
    remains 2D internally, so z=0 is accepted and non-zero z is rejected explicitly.
    """

    if isinstance(value, _base.Vec2):
        return value

    try:
        length = len(value)  # type: ignore[arg-type]
    except (TypeError, AttributeError):
        length = None

    if length in (2, 3):
        try:
            x = float(value[0])  # type: ignore[index]
            y = float(value[1])  # type: ignore[index]
            if length == 3:
                z = float(value[2])  # type: ignore[index]
                if not math.isclose(z, 0.0, abs_tol=1e-12):
                    raise NotImplementedError(
                        "Noon currently supports 2D Manim vectors only; z must be 0"
                    )
            return _base.Vec2(x, y)
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("expected a two- or three-component numeric vector") from error

    raise TypeError("expected a two- or three-component vector")


def linear(t: float) -> float:
    return float(t)


def smooth(t: float) -> float:
    # Public callable mirrors Manim ergonomics. Playback lowers this known function
    # to Noon's deterministic ease_in_out_cubic track easing.
    value = min(max(float(t), 0.0), 1.0)
    return value * value * (3.0 - 2.0 * value)


def _easing_from_rate_func(rate_func: object) -> str:
    if rate_func is linear or rate_func == linear or getattr(rate_func, "__name__", None) == "linear":
        return "linear"
    if rate_func is smooth or rate_func == smooth or getattr(rate_func, "__name__", None) == "smooth":
        return "ease_in_out_cubic"
    raise NotImplementedError(
        "Noon currently supports deterministic rate_func=linear and rate_func=smooth; "
        "arbitrary Python per-frame rate functions are intentionally unsupported"
    )


class VMobject(_BaseMobject):
    """Manim-compatible vector-mobject authoring type over Noon semantic geometry."""

    def copy(self) -> VMobject:
        clone = object.__new__(type(self))
        _BaseMobject.__init__(clone, self._current_raw())
        for name, value in self.__dict__.items():
            if name not in {"_raw", "_scene", "_object"}:
                setattr(clone, name, copy.deepcopy(value))
        return clone


class Circle(VMobject):
    def __init__(
        self,
        radius: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(_ir.Circle(radius, **kwargs))
        self.radius = float(radius)
        if color is not None:
            self.set_color(color)


class Rectangle(VMobject):
    def __init__(
        self,
        width: float = 2.0,
        height: float = 1.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(_ir.Rectangle(width, height, **kwargs))
        self.width_value = float(width)
        self.height_value = float(height)
        if color is not None:
            self.set_color(color)


class Square(Rectangle):
    def __init__(
        self,
        side_length: float = 2.0,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        self.side_length = float(side_length)
        super().__init__(side_length, side_length, color=color, **kwargs)


class Line(VMobject):
    def __init__(
        self,
        start: object = None,
        end: object = None,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        start_value = _base.LEFT if start is None else _as_vec2(start)
        end_value = _base.RIGHT if end is None else _as_vec2(end)
        super().__init__(_ir.Line(start_value, end_value, **kwargs))
        self.start = start_value
        self.end = end_value
        if color is not None:
            self.set_color(color)


class Path(VMobject):
    def __init__(
        self,
        path: _base.VectorPath,
        *,
        color: _base.Color | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(_ir.Path(path, **kwargs))
        self.path = path
        if color is not None:
            self.set_color(color)


class Scene(_BaseScene):
    """Manim-style Scene facade while retaining Noon's compiled scene document."""

    def setup(self) -> None:
        pass

    def construct(self) -> None:
        pass

    def tear_down(self) -> None:
        pass

    def _bind_introducer_target(self, target: object) -> None:
        if isinstance(target, _BaseMobject) and target._scene is None:
            # Use the inherited Noon add path so the object gets a stable identity.
            super().add(target)

    def play(
        self,
        *animations: Any,
        duration: float | None = None,
        run_time: float | None = None,
        start_time: float | None = None,
        easing: str | None = None,
        rate_func: object | None = None,
        **kwargs: Any,
    ) -> Scene:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"unsupported Manim Scene.play option(s): {unsupported}"
            )
        if rate_func is not None and easing is not None:
            raise ValueError("use either rate_func or the low-level easing alias, not both")
        actual_easing = easing or (
            _easing_from_rate_func(rate_func) if rate_func is not None else "linear"
        )

        # Manim introducing animations own the lifecycle transition; users do not
        # need to call add() first. Preserve existing pre-bound Noon objects too.
        for animation in animations:
            if isinstance(animation, (_base.Create, _base.FadeIn)):
                self._bind_introducer_target(animation.target)

        return super().play(
            *animations,
            duration=duration,
            run_time=run_time,
            start_time=start_time,
            easing=actual_easing,
        )


def install() -> None:
    """Install the compatibility surface into the public ``noon`` module."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    # Existing Mobject methods resolve _as_vec2 dynamically from noon.py globals,
    # so replacing that helper makes inherited transforms/layout accept z=0 vectors.
    _base._as_vec2 = _as_vec2

    public = {
        "VMobject": VMobject,
        "Circle": Circle,
        "Rectangle": Rectangle,
        "Square": Square,
        "Line": Line,
        "Path": Path,
        "Scene": Scene,
        "linear": linear,
        "smooth": smooth,
    }
    for name, value in public.items():
        setattr(_base, name, value)

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
