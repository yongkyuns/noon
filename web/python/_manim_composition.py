"""Manim-compatible animation composition backed by Noon's shared Rust scheduler.

This module owns Python class/iterable adaptation only. Child interval geometry is
resolved by ``noon_core::resolve_composition_schedule`` through the WASM bridge.
Composition is recursively lowered to ordinary deterministic Noon tracks.

The current normalized track model can represent an outer composition exactly when
its rate function is linear. Nonlinear outer time warps require a runtime time-map
representation, especially for overlapping/same-property children and reversing
rate functions; those cases fail explicitly rather than being approximated.
"""

from __future__ import annotations

import json
import math
from typing import Any, Iterable

from js import noonResolveCompositionSchedule as _resolve_shared_composition

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat


DEFAULT_LAGGED_START_LAG_RATIO = 0.05
_ORIGINAL_SCENE_PLAY = _compat.Scene.play


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
        self.rate_func = _compat.linear if rate_func is None else rate_func
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


def _resolve_schedule(
    child_run_times: list[float],
    lag_ratio: float,
    run_time: float | None,
):
    if not child_run_times:
        raise ValueError("animation composition requires at least one child")
    result = _resolve_shared_composition(
        json.dumps(child_run_times, separators=(",", ":"), allow_nan=False),
        float(lag_ratio),
        float("nan") if run_time is None else float(run_time),
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))
    return result


def _simple_runtime(animation: object) -> float:
    builder_args = _options.builder_args(animation)
    resolved = _options.resolve(
        builder_args=builder_args,
        default_lag_ratio=_animate._default_lag_ratio(animation),
        play_run_time=None,
        play_easing=None,
        play_rate_func=None,
        play_lag_ratio=None,
    )
    return resolved.run_time


def _composition_local_rate_id(animation: AnimationGroup) -> str:
    return _compat._easing_from_rate_func(animation.rate_func)


def _intrinsic_runtime(animation: object) -> float:
    if not isinstance(animation, AnimationGroup):
        return _simple_runtime(animation)
    child_run_times = [_intrinsic_runtime(child) for child in animation.animations]
    schedule = _resolve_schedule(
        child_run_times,
        animation.lag_ratio,
        animation.run_time,
    )
    return float(schedule.runTime)


def _record_composition_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    if isinstance(animation, AnimationGroup):
        for child in animation.animations:
            _record_composition_wrapper_state(child, states)
        return
    source = _animate._builder_source(animation)
    if source is not None:
        _animate._record_wrapper_state(source, states)
    if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)):
        _animate._record_wrapper_state(animation.target, states)


def _play_leaf(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
) -> None:
    # Explicit run_time here represents the parent composition's resolved child
    # interval. Other animation-local options (including the child rate_func) are
    # still resolved by the ordinary shared AnimationOptions path.
    _ORIGINAL_SCENE_PLAY(
        scene,
        animation,
        run_time=run_time,
        start_time=start_time,
    )


def _play_composition(
    scene: _compat.Scene,
    animation: AnimationGroup,
    *,
    start_time: float,
    run_time_override: float | None,
    rate_func_override: object | None,
    easing_override: str | None,
    lag_ratio_override: float | None,
) -> float:
    if not animation.animations:
        raise ValueError("animation composition requires at least one child")

    if easing_override is not None:
        outer_rate_id = str(easing_override)
    elif rate_func_override is not None:
        outer_rate_id = _compat._easing_from_rate_func(rate_func_override)
    else:
        outer_rate_id = _composition_local_rate_id(animation)

    if outer_rate_id != "linear":
        raise NotImplementedError(
            "nonlinear outer AnimationGroup/LaggedStart/Succession rate_func requires "
            "Noon's runtime composition time-map representation; refusing to flatten "
            f"rate_func={outer_rate_id!r} approximately"
        )

    lag_ratio = (
        animation.lag_ratio
        if lag_ratio_override is None
        else float(lag_ratio_override)
    )
    child_run_times = [_intrinsic_runtime(child) for child in animation.animations]
    requested_run_time = (
        run_time_override
        if run_time_override is not None
        else animation.run_time
    )
    schedule = _resolve_schedule(child_run_times, lag_ratio, requested_run_time)

    for child, interval in zip(animation.animations, schedule.intervals, strict=True):
        child_start = start_time + float(interval.startTime)
        child_duration = float(interval.duration)
        if isinstance(child, AnimationGroup):
            # Parent interval rescaling becomes this nested composition's total
            # runtime. Its own lag ratio/rate function still defines local timing.
            _play_composition(
                scene,
                child,
                start_time=child_start,
                run_time_override=child_duration,
                rate_func_override=None,
                easing_override=None,
                lag_ratio_override=None,
            )
        else:
            _play_leaf(
                scene,
                child,
                start_time=child_start,
                run_time=child_duration,
            )

    return start_time + float(schedule.runTime)


def _composition_scene_play(
    self: _compat.Scene,
    *animations: Any,
    duration: float | None = None,
    run_time: float | None = None,
    start_time: float | None = None,
    easing: str | None = None,
    rate_func: object | None = None,
    lag_ratio: float | None = None,
    **kwargs: Any,
) -> _compat.Scene:
    if not any(isinstance(animation, AnimationGroup) for animation in animations):
        return _ORIGINAL_SCENE_PLAY(
            self,
            *animations,
            duration=duration,
            run_time=run_time,
            start_time=start_time,
            easing=easing,
            rate_func=rate_func,
            lag_ratio=lag_ratio,
            **kwargs,
        )
    if not animations:
        raise ValueError("play requires at least one animation")
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

    play_run_time = run_time if run_time is not None else duration
    if play_run_time is not None:
        play_run_time = float(play_run_time)
    if lag_ratio is not None:
        lag_ratio = float(lag_ratio)
    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")

    checkpoint = self._authoring_checkpoint()
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    wrapper_states: dict[int, tuple[_base.Mobject, object, object]] = {}
    for animation in animations:
        _record_composition_wrapper_state(animation, wrapper_states)

    max_end = base_start
    try:
        for animation in animations:
            if isinstance(animation, AnimationGroup):
                end = _play_composition(
                    self,
                    animation,
                    start_time=base_start,
                    run_time_override=play_run_time,
                    rate_func_override=rate_func,
                    easing_override=easing,
                    lag_ratio_override=lag_ratio,
                )
            else:
                _ORIGINAL_SCENE_PLAY(
                    self,
                    animation,
                    run_time=play_run_time,
                    start_time=base_start,
                    easing=easing,
                    rate_func=rate_func,
                    lag_ratio=lag_ratio,
                )
                end = self._cursor
            max_end = max(max_end, end)

        self._cursor = max(cursor_before, max_end)
        return self
    except Exception:
        self._restore_authoring_checkpoint(checkpoint)
        self._cursor = cursor_before
        self._compat_top_level = top_level_before
        for member, old_scene, old_object in wrapper_states.values():
            member._scene = old_scene
            member._object = old_object
        raise


def install() -> None:
    public = {
        "AnimationGroup": AnimationGroup,
        "LaggedStart": LaggedStart,
        "Succession": Succession,
    }
    for name, value in public.items():
        setattr(_compat, name, value)
        setattr(_base, name, value)

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
    _compat.Scene.play = _composition_scene_play
