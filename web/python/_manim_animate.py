"""Manim-compatible ``.animate`` scheduling semantics.

This layer keeps animation authoring in Python while lowering every supported operation
to Noon's deterministic scene tracks. No Python callbacks run during playback.
"""

from __future__ import annotations

import math
from typing import Any, Callable

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b


class _AnimateBuilderMixin:
    """Mirror Manim's callable/chained ``_AnimationBuilder`` contract."""

    source: object
    target: object
    mobject: object
    anim_args: dict[str, Any]
    cannot_pass_args: bool
    is_chaining: bool

    def _initialize_builder(self, source: object) -> None:
        self.source = source
        self.mobject = source
        self.target = source.copy()  # type: ignore[attr-defined]
        self.anim_args = {}
        self.cannot_pass_args = False
        self.is_chaining = False

    def __call__(self, **kwargs: Any):
        if self.cannot_pass_args:
            raise ValueError(
                "Animation arguments must be passed before accessing methods and can only be passed once"
            )
        self.anim_args = dict(kwargs)
        self.cannot_pass_args = True
        return self

    def __getattr__(self, name: str) -> Callable[..., Any]:
        if name.startswith("_"):
            raise AttributeError(name)
        target_attribute = getattr(self.target, name)
        if not callable(target_attribute):
            raise AttributeError(f"{name} is not an animatable method")

        # Manim prevents animation arguments from being supplied after the first
        # method is accessed, even before the returned method proxy is invoked.
        self.is_chaining = True
        self.cannot_pass_args = True

        def invoke(*args: Any, **kwargs: Any):
            result = target_attribute(*args, **kwargs)
            if result is not None and result is not self.target:
                raise TypeError(
                    f"animate.{name} must be a mutating method returning self or None"
                )
            return self

        return invoke


class _AlignedAnimationBuilder(_AnimateBuilderMixin):
    def __init__(self, source: _base.Mobject) -> None:
        # Unlike the old Noon builder, Manim allows ``self.play(Circle().animate...)``.
        # Binding happens when Scene.play compiles the animation.
        self._initialize_builder(source)


class _AlignedGroupAnimationBuilder(_AnimateBuilderMixin):
    def __init__(self, source: _compat.Group) -> None:
        self._initialize_builder(source)


# Both properties in _manim_compat resolve these globals at access time.
_compat._CompatAnimationBuilder = _AlignedAnimationBuilder
_compat._GroupAnimationBuilder = _AlignedGroupAnimationBuilder


_SUPPORTED_BUILDER_ARGS = {
    "run_time",
    "rate_func",
    "lag_ratio",
    "path_arc",
    "reverse_rate_function",
    "suspend_mobject_updating",
    "name",
}


def _positive_duration(name: str, value: object) -> float:
    result = float(value)
    if not math.isfinite(result) or result <= 0.0:
        raise ValueError(f"{name} must be finite and positive")
    return result


def _lag_ratio(value: object) -> float:
    result = float(value)
    if not math.isfinite(result) or result < 0.0:
        raise ValueError("lag_ratio must be finite and non-negative")
    return result


def _validate_builder_args(builder: object) -> dict[str, Any]:
    args = dict(getattr(builder, "anim_args", {}))
    unsupported = sorted(set(args) - _SUPPORTED_BUILDER_ARGS)
    if unsupported:
        raise NotImplementedError(
            "unsupported Manim .animate option(s): " + ", ".join(unsupported)
        )

    if "path_arc" in args:
        path_arc = float(args["path_arc"])
        if not math.isfinite(path_arc):
            raise ValueError("path_arc must be finite")
        if not math.isclose(path_arc, 0.0, abs_tol=1e-12):
            raise NotImplementedError(
                "non-zero .animate(path_arc=...) requires curved transform paths, "
                "which are not yet represented by Noon's deterministic Transform track"
            )

    if bool(args.get("reverse_rate_function", False)):
        raise NotImplementedError(
            ".animate(reverse_rate_function=True) is not yet represented by Noon's easing IR"
        )

    if "run_time" in args:
        args["run_time"] = _positive_duration("run_time", args["run_time"])
    if "lag_ratio" in args:
        args["lag_ratio"] = _lag_ratio(args["lag_ratio"])
    return args


def _builder_source(animation: object) -> object | None:
    if isinstance(animation, (_AlignedAnimationBuilder, _AlignedGroupAnimationBuilder)):
        return animation.source
    return None


def _record_wrapper_state(value: object, states: dict[int, tuple[_base.Mobject, object, object]]) -> None:
    if isinstance(value, _compat.Group):
        for member in _compat._leaf_mobjects(value):
            _record_wrapper_state(member, states)
        return
    if isinstance(value, _base.Mobject):
        states.setdefault(id(value), (value, value._scene, value._object))


def _bind_for_animation(
    scene: _compat.Scene,
    value: object,
    *,
    start_time: float,
) -> None:
    """Match Manim Scene.play's implicit addition of animated mobjects."""

    leaves = _compat._leaf_mobjects(value)
    for member in leaves:
        newly_bound = member._scene is None
        if newly_bound:
            _phase_b._bind_raw(scene, member)
        elif member._scene is not scene:
            raise ValueError("Mobject already belongs to another Scene")

        assert member._object is not None
        tracks = scene._ensure_lifecycle_timeline_available(
            member._object, start_time, "animated Mobject"
        )
        if (newly_bound and start_time > 0.0) or (
            tracks and not scene._presence_at(member._object, start_time)
        ):
            scene._add_presence_track(
                member._object,
                False,
                True,
                start_time,
                key=f"@scene-play-add:{member._object.id}:{start_time:g}",
            )

    scene._register_top_level(value)


def _default_lag_ratio(animation: object) -> float:
    # Manim Create defaults to lag_ratio=1; ordinary Transform/_MethodAnimation
    # and fading animations default to zero.
    if isinstance(animation, _base.Create) and isinstance(animation.target, _compat.Group):
        return 1.0
    return 0.0


def _resolve_easing(
    *,
    builder_args: dict[str, Any],
    play_easing: str | None,
    play_rate_func: object | None,
) -> str:
    if play_easing is not None:
        return play_easing
    if play_rate_func is not None:
        return _compat._easing_from_rate_func(play_rate_func)
    if "rate_func" in builder_args:
        return _compat._easing_from_rate_func(builder_args["rate_func"])
    # Manim Animation defaults to rate_func=smooth. Noon currently lowers the
    # known function to its deterministic ease-in-out easing representation.
    return _compat._easing_from_rate_func(_compat.smooth)


def _expanded_schedule(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
    easing: str,
    lag_ratio: float,
) -> list[tuple[object, float, float, str]]:
    if isinstance(animation, _AlignedAnimationBuilder):
        expanded = [_base.Transform(animation.source, animation.target)]
    else:
        expanded = scene._expand_animation(animation)
    if not expanded:
        return []

    is_family_animation = isinstance(animation, _AlignedGroupAnimationBuilder) or (
        isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut))
        and isinstance(animation.target, _compat.Group)
    )
    if not is_family_animation or len(expanded) == 1:
        return [(item, start_time, run_time, easing) for item in expanded]

    # This is Manim Animation.get_sub_alpha's timing geometry written directly as
    # deterministic child intervals: full_length = 1 + (n-1)*lag_ratio.
    full_length = 1.0 + (len(expanded) - 1) * lag_ratio
    child_duration = run_time / full_length
    offset = lag_ratio * child_duration
    return [
        (item, start_time + index * offset, child_duration, easing)
        for index, item in enumerate(expanded)
    ]


def _aligned_scene_play(
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
    if not animations:
        raise ValueError("play requires at least one animation")
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

    play_duration = None
    if run_time is not None:
        play_duration = _positive_duration("run_time", run_time)
    elif duration is not None:
        play_duration = _positive_duration("duration", duration)
    play_lag_ratio = None if lag_ratio is None else _lag_ratio(lag_ratio)

    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")

    checkpoint = self._authoring_checkpoint()
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    wrapper_states: dict[int, tuple[_base.Mobject, object, object]] = {}
    for animation in animations:
        source = _builder_source(animation)
        if source is not None:
            _record_wrapper_state(source, wrapper_states)
        if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)):
            _record_wrapper_state(animation.target, wrapper_states)

    max_end = base_start
    try:
        # Introducing animations bind their target; method animations and FadeOut
        # match Manim's normal Scene.play behavior by implicitly adding their mobject.
        for animation in animations:
            source = _builder_source(animation)
            if source is not None:
                _bind_for_animation(self, source, start_time=base_start)
            elif isinstance(animation, (_base.Create, _base.FadeIn)):
                self._bind_introducer_target(animation.target)
            elif isinstance(animation, _base.FadeOut):
                _bind_for_animation(self, animation.target, start_time=base_start)

        for animation in animations:
            builder_args = _validate_builder_args(animation)
            item_duration = (
                play_duration
                if play_duration is not None
                else builder_args.get("run_time", 1.0)
            )
            item_duration = _positive_duration("run_time", item_duration)
            item_easing = _resolve_easing(
                builder_args=builder_args,
                play_easing=easing,
                play_rate_func=rate_func,
            )
            item_lag_ratio = (
                play_lag_ratio
                if play_lag_ratio is not None
                else builder_args.get("lag_ratio", _default_lag_ratio(animation))
            )
            item_lag_ratio = _lag_ratio(item_lag_ratio)

            schedule = _expanded_schedule(
                self,
                animation,
                start_time=base_start,
                run_time=item_duration,
                easing=item_easing,
                lag_ratio=item_lag_ratio,
            )
            for lowered, child_start, child_duration, child_easing in schedule:
                _base.Scene.play(
                    self,
                    lowered,
                    run_time=child_duration,
                    start_time=child_start,
                    easing=child_easing,
                )
            max_end = max(max_end, base_start + item_duration)

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


_compat.Scene.play = _aligned_scene_play
