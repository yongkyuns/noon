"""Manim-compatible animation composition backed by Noon's shared Rust scheduler.

This module owns Python class/iterable adaptation only. Child interval geometry is
resolved by ``noon_core::resolve_composition_schedule`` through the WASM bridge.
Composition is recursively lowered to ordinary deterministic Noon tracks. When a
composition contains a nonlinear outer rate function, leaf tracks carry the shared
core ``CompositionTimeMap`` representation instead of approximating the warp in
Python or executing a frontend callback during playback.
"""

from __future__ import annotations

import copy
import json
import math
from typing import Any, Callable, Iterable

from js import noonResolveCompositionSchedule as _resolve_shared_composition

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat


DEFAULT_LAGGED_START_LAG_RATIO = 0.05
_ORIGINAL_SCENE_PLAY = _compat.Scene.play


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
        self.rate_func = _compat.linear if rate_func is None else rate_func
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
    if isinstance(animation, (Wait, Add)):
        _options.builder_args(animation)
        return animation.run_time
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
    if isinstance(animation, Add):
        for member in _compat._leaf_mobjects(animation.mobject):
            _animate._record_wrapper_state(member, states)
        return
    source = _animate._builder_source(animation)
    if source is not None:
        _animate._record_wrapper_state(source, states)
    if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)):
        _animate._record_wrapper_state(animation.target, states)


def _normalized_time_map_step(interval: object, run_time: float, rate_id: str) -> dict[str, Any]:
    if not math.isfinite(run_time) or run_time <= 0.0:
        raise ValueError("composition run_time must be finite and positive")
    return {
        "start": float(interval.startTime) / run_time,
        "duration": float(interval.duration) / run_time,
        "rate_func": rate_id,
    }


def _path_requires_time_map(steps: list[dict[str, Any]]) -> bool:
    return any(step["rate_func"] != "linear" for step in steps)


def _schedule_add(scene: _compat.Scene, animation: Add, *, start_time: float) -> None:
    # Import lazily: the lifecycle layer is installed after this module during
    # worker bootstrap, while Add is only scheduled after bootstrap completes.
    import _manim_lifecycle as _lifecycle
    import _manim_phase_b as _phase_b

    start = float(start_time)
    for member in _compat._leaf_mobjects(animation.mobject):
        plan = _lifecycle._resolve_wrapper(
            scene,
            member,
            "add",
            start,
            "Add target",
        )
        if plan.bind:
            _phase_b._bind_raw(scene, member)
        assert member._object is not None
        if plan.show_now:
            scene._add_presence_track(
                member._object,
                False,
                True,
                start,
                key=f"@add:{member._object.id}:{start:g}",
            )
    scene._register_top_level(animation.mobject)


def _play_leaf(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
    time_map_steps: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]],
) -> None:
    if isinstance(animation, Wait):
        return
    if isinstance(animation, Add):
        _schedule_add(scene, animation, start_time=start_time)
        return

    # Author the leaf first at its flattened interval. This preserves the existing
    # deterministic target-state/lifecycle checks and lets successive animations of
    # the same object build their `from` snapshots in virtual order. Once the full
    # composition has been authored, nonlinear paths are rewritten to the root
    # interval and tagged with the shared core time map.
    track_start = len(scene._tracks)
    _ORIGINAL_SCENE_PLAY(
        scene,
        animation,
        run_time=run_time,
        start_time=start_time,
    )
    track_end = len(scene._tracks)
    if track_end > track_start and _path_requires_time_map(time_map_steps):
        pending_time_maps.append(
            (track_start, track_end, copy.deepcopy(time_map_steps))
        )


def _schedule_composition(
    scene: _compat.Scene,
    animation: AnimationGroup,
    *,
    start_time: float,
    run_time_override: float | None,
    rate_func_override: object | None,
    easing_override: str | None,
    lag_ratio_override: float | None,
    time_map_steps: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]],
) -> float:
    if not animation.animations:
        raise ValueError("animation composition requires at least one child")

    if easing_override is not None:
        outer_rate_id = str(easing_override)
    elif rate_func_override is not None:
        outer_rate_id = _compat._easing_from_rate_func(rate_func_override)
    else:
        outer_rate_id = _composition_local_rate_id(animation)

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
    schedule_run_time = float(schedule.runTime)

    for child, interval in zip(animation.animations, schedule.intervals, strict=True):
        child_start = start_time + float(interval.startTime)
        child_duration = float(interval.duration)
        child_steps = list(time_map_steps)
        if schedule_run_time > 0.0:
            child_steps.append(
                _normalized_time_map_step(interval, schedule_run_time, outer_rate_id)
            )
        if isinstance(child, AnimationGroup):
            _schedule_composition(
                scene,
                child,
                start_time=child_start,
                run_time_override=child_duration if child_duration > 0.0 else None,
                rate_func_override=None,
                easing_override=None,
                lag_ratio_override=None,
                time_map_steps=child_steps,
                pending_time_maps=pending_time_maps,
            )
        else:
            _play_leaf(
                scene,
                child,
                start_time=child_start,
                run_time=child_duration,
                time_map_steps=child_steps,
                pending_time_maps=pending_time_maps,
            )

    return schedule_run_time


def _apply_pending_time_maps(
    scene: _compat.Scene,
    pending: list[tuple[int, int, list[dict[str, Any]]]],
    *,
    root_start: float,
    root_run_time: float,
) -> None:
    for track_start, track_end, steps in pending:
        time_map = {"steps": copy.deepcopy(steps)}
        for track in scene._tracks[track_start:track_end]:
            # Presence remains an instant lifecycle event and intentionally cannot
            # carry a continuous time map. The animated leaf tracks are mapped;
            # cleanup events continue to use the deterministic authored timeline.
            if track["property"] == "presence":
                continue
            track["timing"]["start_time"] = root_start
            track["timing"]["duration"] = root_run_time
            track["time_map"] = copy.deepcopy(time_map)


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
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] = []
    root_run_time = _schedule_composition(
        scene,
        animation,
        start_time=start_time,
        run_time_override=run_time_override,
        rate_func_override=rate_func_override,
        easing_override=easing_override,
        lag_ratio_override=lag_ratio_override,
        time_map_steps=[],
        pending_time_maps=pending_time_maps,
    )
    _apply_pending_time_maps(
        scene,
        pending_time_maps,
        root_start=start_time,
        root_run_time=root_run_time,
    )
    return start_time + root_run_time


def _custom_leaf_end(
    animation: Wait | Add,
    *,
    start_time: float,
    run_time_override: float | None,
) -> float:
    run_time = animation.run_time if run_time_override is None else float(run_time_override)
    run_time = _nonnegative_run_time(run_time, type(animation).__name__)
    return start_time + run_time


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
    if not any(isinstance(animation, (AnimationGroup, Wait, Add)) for animation in animations):
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
        if not math.isfinite(play_run_time) or play_run_time < 0.0:
            raise ValueError("run_time must be finite and non-negative")
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
            elif isinstance(animation, Add):
                _options.builder_args(animation)
                _schedule_add(self, animation, start_time=base_start)
                end = _custom_leaf_end(
                    animation,
                    start_time=base_start,
                    run_time_override=play_run_time,
                )
            elif isinstance(animation, Wait):
                _options.builder_args(animation)
                end = _custom_leaf_end(
                    animation,
                    start_time=base_start,
                    run_time_override=play_run_time,
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
        "Add": Add,
        "AnimationGroup": AnimationGroup,
        "LaggedStart": LaggedStart,
        "LaggedStartMap": LaggedStartMap,
        "Succession": Succession,
        "Wait": Wait,
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
