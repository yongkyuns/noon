"""Manim-compatible ``.animate`` scheduling semantics.

This layer keeps Python syntax adaptation while lowering supported operations to Noon's
deterministic scene tracks. Animation option defaults, validation, precedence, and
composition timing are resolved by shared Rust authoring semantics; no Python callbacks
run during playback.
"""

from __future__ import annotations

import copy
import math
from typing import Any, Callable

from js import noonResolveUniformCompositionSchedule as _resolve_uniform_composition_schedule

import noon as _base
import _manim_animation_options as _options
import _manim_compat as _compat
import _manim_phase_b as _phase_b
import _manim_rate_functions as _rate_functions
import _manim_semantic_handles as _semantic_handles


_ORIGINAL_TRANSFORM = _base.Transform
_ORIGINAL_REPLACEMENT_TRANSFORM = _base.ReplacementTransform
_ORIGINAL_TRANSFORM_FROM_COPY = _base.TransformFromCopy
_ORIGINAL_TRANSFORM_MATCHING_SHAPES = _base.TransformMatchingShapes
_ORIGINAL_CREATE = _base.Create
_ORIGINAL_UNCREATE = _base.Uncreate
_ORIGINAL_FADE_IN = _base.FadeIn
_ORIGINAL_FADE_OUT = _base.FadeOut
_PURE_YELLOW = _base.color_from_hex("#FFFF00")


def _store_animation_args(animation: object, kwargs: dict[str, Any]) -> None:
    """Attach Manim Animation kwargs for the shared option resolver.

    Noon's original animation records are frozen/slotted dataclasses. Subclassing them
    gives the compatibility facade a small Python ``__dict__`` for authoring metadata
    while preserving the existing low-level fields and isinstance-based lowering.
    Validation remains centralized in ``_manim_animation_options`` at play time.
    """

    object.__setattr__(animation, "anim_args", dict(kwargs))


def _fade_authoring_options(
    target: object, kwargs: dict[str, Any]
) -> tuple[dict[str, Any], _base.Vec2, float, bool]:
    """Separate `_Fade` endpoint options from generic Animation options.

    Manim resolves ``target_position`` during `_Fade.__init__`, while an explicitly
    supplied ``shift`` takes precedence over it. Preserve that authoring-time behavior
    and leave generic timing/rate options for the shared Rust option resolver.
    """

    if not isinstance(target, (_base.Mobject, _compat.Group)):
        raise TypeError("FadeIn/FadeOut target must be a Mobject or Group")

    animation_kwargs = dict(kwargs)
    shift = animation_kwargs.pop("shift", None)
    target_position = animation_kwargs.pop("target_position", None)
    scale_factor = float(animation_kwargs.pop("scale", 1.0))
    if not math.isfinite(scale_factor):
        raise ValueError("fade scale must be finite")

    point_target = False
    if shift is not None:
        shift_vector = _base._as_vec2(shift)
    elif target_position is not None:
        if isinstance(target_position, (_base.Mobject, _compat.Group)):
            point = target_position.get_center()
        else:
            point = _base._as_vec2(target_position)
        shift_vector = point - target.get_center()
        point_target = True
    else:
        shift_vector = _base.ORIGIN

    return animation_kwargs, shift_vector, scale_factor, point_target


def _store_fade_options(
    animation: object,
    *,
    shift_vector: _base.Vec2,
    scale_factor: float,
    point_target: bool,
) -> None:
    object.__setattr__(animation, "_fade_shift_vector", shift_vector)
    object.__setattr__(animation, "_fade_scale_factor", scale_factor)
    object.__setattr__(animation, "_fade_point_target", point_target)


class Transform(_ORIGINAL_TRANSFORM):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class Indicate:
    """Inert ManimCE ``Indicate`` request for shared Rust semantic playback."""

    def __init__(
        self,
        mobject: object,
        scale_factor: float = 1.2,
        color: _base.Color = _PURE_YELLOW,
        rate_func: object = _rate_functions.there_and_back,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Indicate target must be a Mobject or Group")
        factor = float(scale_factor)
        if not math.isfinite(factor):
            raise ValueError("Indicate scale_factor must be finite")

        self.mobject = mobject
        self.scale_factor = factor
        self.color = color
        animation_kwargs = dict(kwargs)
        animation_kwargs["rate_func"] = rate_func
        _store_animation_args(self, animation_kwargs)


class ReplacementTransform(_ORIGINAL_REPLACEMENT_TRANSFORM):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class TransformFromCopy(_ORIGINAL_TRANSFORM_FROM_COPY):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class TransformMatchingShapes(_ORIGINAL_TRANSFORM_MATCHING_SHAPES):
    def __init__(
        self,
        sources: object,
        targets: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(sources, targets, key)
        _store_animation_args(self, kwargs)


class Create(_ORIGINAL_CREATE):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        super().__init__(target, key)
        _store_animation_args(self, kwargs)


class Uncreate(_ORIGINAL_UNCREATE):
    def __init__(
        self,
        target: object,
        key: str | None = None,
        reverse_rate_function: bool = True,
        remover: bool = True,
        **kwargs: Any,
    ) -> None:
        super().__init__(target, key, bool(reverse_rate_function), bool(remover))
        _store_animation_args(self, kwargs)


class FadeIn(_ORIGINAL_FADE_IN):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        animation_kwargs, shift_vector, scale_factor, point_target = _fade_authoring_options(
            target, kwargs
        )
        super().__init__(target, key)
        _store_animation_args(self, animation_kwargs)
        _store_fade_options(
            self,
            shift_vector=shift_vector,
            scale_factor=scale_factor,
            point_target=point_target,
        )


class FadeOut(_ORIGINAL_FADE_OUT):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        animation_kwargs, shift_vector, scale_factor, point_target = _fade_authoring_options(
            target, kwargs
        )
        super().__init__(target, key)
        _store_animation_args(self, animation_kwargs)
        _store_fade_options(
            self,
            shift_vector=shift_vector,
            scale_factor=scale_factor,
            point_target=point_target,
        )


# Replace only the public compatibility classes. Their frozen Noon bases remain the
# low-level representation and existing code using isinstance(..., noon.Create/etc.)
# continues to work because the module globals now point at these subclasses.
for _name, _value in {
    "Transform": Transform,
    "Indicate": Indicate,
    "ReplacementTransform": ReplacementTransform,
    "TransformFromCopy": TransformFromCopy,
    "TransformMatchingShapes": TransformMatchingShapes,
    "Create": Create,
    "Uncreate": Uncreate,
    "FadeIn": FadeIn,
    "FadeOut": FadeOut,
}.items():
    setattr(_base, _name, _value)

if "Indicate" not in _base.__all__:
    _base.__all__.append("Indicate")


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
        target_factory = getattr(source, "_copy_for_animate_target", None)
        self.target = (
            target_factory()
            if target_factory is not None
            else source.copy()  # type: ignore[attr-defined]
        )
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


def _builder_source(animation: object) -> object | None:
    if isinstance(animation, Indicate):
        return animation.mobject
    if isinstance(animation, (_AlignedAnimationBuilder, _AlignedGroupAnimationBuilder)):
        return animation.source
    return None


def _record_wrapper_state(
    value: object, states: dict[int, tuple[_base.Mobject, object, object]]
) -> None:
    if isinstance(value, _compat.Group):
        for member in _compat._leaf_mobjects(value):
            _record_wrapper_state(member, states)
        return
    if isinstance(value, _base.Mobject):
        states.setdefault(id(value), (value, value._scene, value._object))


def _record_animation_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    """Capture wrappers that ordinary play binding may mutate.

    Mixed family/ordinary play reuses this exact ownership boundary so one outer
    transaction can restore wrappers without reimplementing the ordinary scheduler.
    """

    source = _builder_source(animation)
    if source is not None:
        _record_wrapper_state(source, states)
    if isinstance(animation, (_base.Create, _base.Uncreate, _base.FadeIn, _base.FadeOut)):
        _record_wrapper_state(animation.target, states)


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


def _prepare_aligned_animation_binding(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
) -> None:
    """Bind one ordinary animation without scheduling or committing target state."""

    source = _builder_source(animation)
    if source is not None:
        _bind_for_animation(scene, source, start_time=start_time)
    elif isinstance(animation, _base.Uncreate):
        _bind_for_animation(scene, animation.target, start_time=start_time)
    elif isinstance(animation, (_base.Create, _base.FadeIn)):
        scene._bind_introducer_target(animation.target)
    elif isinstance(animation, _base.FadeOut):
        _bind_for_animation(scene, animation.target, start_time=start_time)


def _prepare_aligned_bindings(
    scene: _compat.Scene,
    animations: tuple[object, ...] | list[object],
    *,
    start_time: float,
) -> None:
    for animation in animations:
        _prepare_aligned_animation_binding(scene, animation, start_time=start_time)


def _uncreate_track_settings(easing: str, reverse_rate_function: bool) -> tuple[str, bool]:
    """Represent Manim's `rate_func(1 - alpha)` using Noon's scalar track.

    Most supported rate functions can be expressed by reversing the reveal endpoints
    and using the complement-reversed easing. `there_and_back` is time-symmetric and
    therefore keeps the forward endpoints instead.
    """
    if not reverse_rate_function:
        return easing, False
    if easing in {"linear", "smooth", "ease_in_out_cubic"}:
        return easing, True
    if easing == "rush_into":
        return "rush_from", True
    if easing == "rush_from":
        return "rush_into", True
    if easing == "there_and_back":
        return easing, False
    raise NotImplementedError(f"cannot reverse unsupported rate function {easing!r}")


def _default_lag_ratio(animation: object) -> float:
    # Manim Create defaults to lag_ratio=1; ordinary Transform/_MethodAnimation
    # and fading animations default to zero.
    if isinstance(animation, _base.Create) and isinstance(animation.target, _compat.Group):
        return 1.0
    return 0.0


def _shared_uniform_intervals(
    child_count: int,
    *,
    lag_ratio: float,
    run_time: float,
) -> list[tuple[float, float]]:
    result = _resolve_uniform_composition_schedule(
        int(child_count), float(lag_ratio), float(run_time)
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))
    intervals = result.intervals
    return [
        (
            float(intervals[index].startTime),
            float(intervals[index].duration),
        )
        for index in range(int(intervals.length))
    ]


def _expanded_schedule(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
    easing: str,
    lag_ratio: float,
) -> list[tuple[object, float, float, str]]:
    if isinstance(animation, (_AlignedGroupAnimationBuilder, Indicate)):
        raise NotImplementedError(
            "family Transform and Indicate require shared semantic composition playback"
        )
    if isinstance(animation, _AlignedAnimationBuilder):
        expanded = [_base.Transform(animation.source, animation.target)]
    else:
        expanded = scene._expand_animation(animation)
    if not expanded:
        return []

    is_family_animation = isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)) and isinstance(animation.target, _compat.Group)
    if not is_family_animation or len(expanded) == 1:
        return [(item, start_time, run_time, easing) for item in expanded]

    intervals = _shared_uniform_intervals(
        len(expanded), lag_ratio=lag_ratio, run_time=run_time
    )
    return [
        (item, start_time + child_start, child_duration, easing)
        for item, (child_start, child_duration) in zip(expanded, intervals)
    ]


def _snapshot_mobject(snapshot: dict[str, Any]) -> _base.Mobject:
    return _base.Mobject(
        _base._ir.Mobject(
            geometry=copy.deepcopy(snapshot["geometry"]),
            transform=copy.deepcopy(snapshot["transform"]),
            style=copy.deepcopy(snapshot["style"]),
        )
    )


def _fade_endpoint_snapshots(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
) -> dict[int, dict[str, Any]]:
    """Build Manim's faded copy endpoint for each leaf at play start."""

    if not isinstance(animation, (FadeIn, FadeOut)):
        return {}

    shift_vector = animation._fade_shift_vector
    scale_factor = animation._fade_scale_factor
    if shift_vector == _base.ORIGIN and math.isclose(scale_factor, 1.0, abs_tol=1e-15):
        return {}

    leaves = _compat._leaf_mobjects(animation.target)
    temporary: list[_base.Mobject] = []
    for member in leaves:
        if member._object is None:
            raise ValueError("fade target must be bound before endpoint construction")
        snapshot = scene._snapshot_for_object_at(member._object, start_time)
        temporary.append(_snapshot_mobject(snapshot))

    endpoint: object
    if isinstance(animation.target, _compat.Group):
        endpoint = _compat.Group(*temporary)
    else:
        endpoint = temporary[0]

    direction_modifier = -1.0 if isinstance(animation, FadeIn) and not animation._fade_point_target else 1.0
    if shift_vector != _base.ORIGIN:
        endpoint.shift(shift_vector * direction_modifier)  # type: ignore[attr-defined]
    if not math.isclose(scale_factor, 1.0, abs_tol=1e-15):
        endpoint.scale(scale_factor)  # type: ignore[attr-defined]

    return {
        id(member): faded.to_ir()
        for member, faded in zip(leaves, temporary)
    }


def _schedule_fade_endpoint_transform(
    scene: _compat.Scene,
    animation: object,
    member: _base.Mobject,
    faded_snapshot: dict[str, Any],
    *,
    duration: float,
    start_time: float,
    easing: str,
    key: str | None,
) -> None:
    """Lower Fade shift/scale to the ordinary deterministic Transform channel."""

    if member._object is None:
        raise ValueError("fade target must be bound before endpoint scheduling")
    obj = member._object
    previous_end = scene._scheduled_transform_ends.get(obj.id)
    if previous_end is not None and start_time < previous_end:
        raise ValueError("generic Transform tracks for one object must not overlap")

    current_snapshot = scene._snapshot_for_object_at(obj, start_time)
    fade_in = isinstance(animation, FadeIn)
    from_snapshot = faded_snapshot if fade_in else current_snapshot
    to_snapshot = current_snapshot if fade_in else faded_snapshot

    object_key = scene._object_keys[obj.id]
    direction = "in" if fade_in else "out"
    transform_key = (
        f"{key}.transform"
        if key is not None
        else f"@fade-{direction}:{object_key}:{start_time:g}.transform"
    )
    scene._add_track(
        obj,
        "transform",
        {
            "object": {
                "from": copy.deepcopy(from_snapshot),
                "to": copy.deepcopy(to_snapshot),
            }
        },
        start_time,
        duration,
        easing,
        transform_key,
    )
    scene._scheduled_transform_targets[obj.id] = copy.deepcopy(to_snapshot)
    scene._scheduled_transform_ends[obj.id] = start_time + duration


def _schedule_aligned_bound_animations(
    scene: _compat.Scene,
    animations: tuple[object, ...] | list[object],
    *,
    base_start: float,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
) -> tuple[float, dict[int, tuple[_base.Mobject, _base.Mobject]]]:
    """Schedule already-bound ordinary animations without committing semantic targets.

    Keeping semantic-handle commit outside this phase lets a higher-level mixed play
    transaction include retained family requests without leaving partially advanced
    authoring handles if any sibling animation fails.
    """

    max_end = base_start
    semantic_targets: dict[int, tuple[_base.Mobject, _base.Mobject]] = {}
    for animation in animations:
        builder_args = _options.builder_args(animation)
        resolved = _options.resolve(
            builder_args=builder_args,
            default_lag_ratio=_default_lag_ratio(animation),
            play_run_time=play_run_time,
            play_easing=play_easing,
            play_rate_func=play_rate_func,
            play_lag_ratio=play_lag_ratio,
        )

        fade_snapshots = _fade_endpoint_snapshots(
            scene, animation, start_time=base_start
        )
        schedule = _expanded_schedule(
            scene,
            animation,
            start_time=base_start,
            run_time=resolved.run_time,
            easing=resolved.rate_func,
            lag_ratio=resolved.lag_ratio,
        )
        for lowered, child_start, child_duration, child_easing in schedule:
            if isinstance(animation, (FadeIn, FadeOut)) and isinstance(
                lowered, (_base.FadeIn, _base.FadeOut)
            ):
                member = lowered.target
                faded_snapshot = fade_snapshots.get(id(member))
                if faded_snapshot is not None:
                    _schedule_fade_endpoint_transform(
                        scene,
                        animation,
                        member,
                        faded_snapshot,
                        duration=child_duration,
                        start_time=child_start,
                        easing=child_easing,
                        key=lowered.key,
                    )

            if isinstance(lowered, _base.Uncreate):
                child_easing, track_reverse = _uncreate_track_settings(
                    child_easing, lowered.reverse_rate_function
                )
                lowered = type(lowered)(
                    lowered.target,
                    lowered.key,
                    reverse_rate_function=track_reverse,
                    remover=lowered.remover,
                )

            if isinstance(lowered, _base.Transform):
                source = lowered.source
                target = lowered.target
                if isinstance(source, _base.Mobject) and isinstance(target, _base.Mobject):
                    semantic_targets[id(source)] = (source, target)

            # `noon.Scene` has already been replaced by the compatibility class
            # during install, so use the original captured facade explicitly to
            # avoid recursively re-entering this compatibility scheduler.
            _compat._BaseScene.play(
                scene,
                lowered,
                run_time=child_duration,
                start_time=child_start,
                easing=child_easing,
            )
        max_end = max(max_end, base_start + resolved.run_time)

    return max_end, semantic_targets


def _commit_semantic_targets(
    semantic_targets: dict[int, tuple[_base.Mobject, _base.Mobject]],
) -> None:
    for source, target in semantic_targets.values():
        _semantic_handles.commit_transform_target(source, target)


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
        _record_animation_wrapper_state(animation, wrapper_states)

    try:
        _prepare_aligned_bindings(self, list(animations), start_time=base_start)
        max_end, semantic_targets = _schedule_aligned_bound_animations(
            self,
            list(animations),
            base_start=base_start,
            play_run_time=play_run_time,
            play_easing=easing,
            play_rate_func=rate_func,
            play_lag_ratio=lag_ratio,
        )
        self._cursor = max(cursor_before, max_end)
        _commit_semantic_targets(semantic_targets)
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
