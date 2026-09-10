"""Inert Manim composition requests for the shared Rust animation operation.

Python owns class shape and iterable/argument coercion. Rust owns child timing,
recursive scheduling, lifecycle, time maps, publication, and execution.
"""

from __future__ import annotations

import math
from typing import Any, Callable, Iterable

import noon as _base
import _manim_compat as _compat
import _manim_rate_functions as _rate_functions


DEFAULT_LAGGED_START_LAG_RATIO = 0.05


def _nonnegative_run_time(value: object, label: str) -> float:
    run_time = float(value)
    if not math.isfinite(run_time) or run_time < 0.0:
        raise ValueError(f"{label} run_time must be finite and non-negative")
    return run_time


class Wait:
    """Manim-compatible no-op animation that only occupies timeline duration."""

    def __init__(
        self,
        run_time: float = 1.0,
        stop_condition: Callable[[], bool] | None = None,
        frozen_frame: bool | None = None,
        rate_func: object = None,
        **kwargs: Any,
    ) -> None:
        if stop_condition is not None:
            raise NotImplementedError(
                "Wait(stop_condition=...) requires runtime polling and is not deterministic"
            )
        self.run_time = _nonnegative_run_time(run_time, "Wait")
        self.stop_condition = None
        self.frozen_frame = frozen_frame
        self.rate_func = _rate_functions.linear if rate_func is None else rate_func
        self.anim_args = dict(kwargs)
        if rate_func is not None:
            self.anim_args["rate_func"] = rate_func


class Add:
    """Introduce one or more mobjects at an exact authored timeline instant."""

    def __init__(self, *mobjects: object, run_time: float = 0.0, **kwargs: Any) -> None:
        if not mobjects:
            raise ValueError("Add requires at least one Mobject")
        for mobject in mobjects:
            if not isinstance(mobject, (_base.Mobject, _compat.Group)):
                raise TypeError("Add targets must be Mobjects or Groups")
        self.mobjects = tuple(mobjects)
        self.mobject = mobjects[0] if len(mobjects) == 1 else _compat.Group(*mobjects)
        self.run_time = _nonnegative_run_time(run_time, "Add")
        self.anim_args = dict(kwargs)


def _flatten_animations(values: Iterable[object]) -> list[object]:
    flattened: list[object] = []
    for value in values:
        if isinstance(value, (list, tuple)):
            flattened.extend(_flatten_animations(value))
        else:
            flattened.append(value)
    return flattened


class AnimationGroup:
    """Play child animations using Manim-compatible composition timing."""

    def __init__(
        self,
        *animations: object,
        group: object | None = None,
        run_time: float | None = None,
        rate_func: object = None,
        lag_ratio: float = 0.0,
        **kwargs: Any,
    ) -> None:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"unsupported AnimationGroup option(s): {unsupported}"
            )
        self.animations = _flatten_animations(animations)
        self.group = group
        self.run_time = None if run_time is None else float(run_time)
        self.rate_func = _rate_functions.linear if rate_func is None else rate_func
        self.lag_ratio = float(lag_ratio)
        if not math.isfinite(self.lag_ratio) or self.lag_ratio < 0.0:
            raise ValueError("lag_ratio must be finite and non-negative")
        if self.run_time is not None and (
            not math.isfinite(self.run_time) or self.run_time <= 0.0
        ):
            raise ValueError("run_time must be finite and positive")


class Succession(AnimationGroup):
    def __init__(self, *animations: object, lag_ratio: float = 1.0, **kwargs: Any):
        super().__init__(*animations, lag_ratio=lag_ratio, **kwargs)


class LaggedStart(AnimationGroup):
    def __init__(
        self,
        *animations: object,
        lag_ratio: float = DEFAULT_LAGGED_START_LAG_RATIO,
        **kwargs: Any,
    ) -> None:
        super().__init__(*animations, lag_ratio=lag_ratio, **kwargs)


class LaggedStartMap(LaggedStart):
    """Apply one animation constructor to every direct child of a group."""

    def __init__(
        self,
        animation_class: Callable[..., object],
        mobject: object,
        arg_creator: Callable[[object], object] | None = None,
        run_time: float = 2.0,
        lag_ratio: float = DEFAULT_LAGGED_START_LAG_RATIO,
        **kwargs: Any,
    ) -> None:
        if not callable(animation_class):
            raise TypeError("animation_class must be callable")
        try:
            members = list(mobject)  # type: ignore[arg-type]
        except TypeError as error:
            raise TypeError("LaggedStartMap mobject must be iterable") from error

        animation_kwargs = dict(kwargs)
        animation_kwargs.pop("lag_ratio", None)
        animations: list[object] = []
        for member in members:
            created = member if arg_creator is None else arg_creator(member)
            if isinstance(created, (_base.Mobject, _compat.Group)):
                args = (created,)
            else:
                try:
                    args = tuple(created)  # type: ignore[arg-type]
                except TypeError:
                    args = (created,)
            animations.append(animation_class(*args, **animation_kwargs))

        super().__init__(*animations, run_time=run_time, lag_ratio=lag_ratio)
