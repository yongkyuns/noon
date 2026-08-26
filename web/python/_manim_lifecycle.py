"""Thin Python adapter for Noon's shared Rust lifecycle authoring semantics.

Python owns wrapper identity and emits the canonical scene operations requested by
Rust. Presence legality, reintroduction/removal rules, source/target requirements,
and presence-chain validation are resolved by the shared core planner.

The module also owns the deterministic subset-display creation slice. Manim's
``ShowIncreasingSubsets`` and ``ShowSubmobjectsOneByOne`` overwrite direct-child
fill/stroke opacity at exact threshold instants. Noon lowers those discontinuities to
retained object-snapshot tracks using shared ``step_start`` / ``step_end`` rate
functions; no Python callback participates in playback.
"""

from __future__ import annotations

import copy
import math
from dataclasses import dataclass
from typing import Any

from js import noonResolveLifecyclePlan as _resolve_shared_lifecycle
from js import noonValidatePresenceTransition as _validate_shared_presence_transition

import noon as _base
import _noon_ir as _ir
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_composition as _composition
import _manim_phase_b as _phase_b
import _manim_rate_functions as _rate_functions

_INSTALLED = False
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_COMPOSITION_PLAY_LEAF = _composition._play_leaf
_ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE = _composition._record_composition_wrapper_state


@dataclass(frozen=True)
class LifecyclePlan:
    bind: bool
    show_now: bool
    hide_now: bool
    show_at_start: bool
    hide_at_end: bool


def _resolve(
    intent: str,
    *,
    binding: str,
    has_presence_timeline: bool,
    present: bool,
    has_future_event: bool,
    at_time_zero: bool,
    label: str,
) -> LifecyclePlan:
    result = _resolve_shared_lifecycle(
        intent,
        binding,
        bool(has_presence_timeline),
        bool(present),
        bool(has_future_event),
        bool(at_time_zero),
    )
    if not bool(result.ok):
        kind = str(result.errorKind)
        if kind == "requires_present":
            raise ValueError(f"{label} must be present at animation start")
        if kind == "requires_absent":
            raise ValueError(f"{label} must be absent before introduction or handoff")
        if kind == "future_event":
            raise ValueError(
                f"{label} has a future lifecycle event; lifecycle operations must be authored chronologically"
            )
        raise ValueError(str(result.message))
    return LifecyclePlan(
        bind=bool(result.bind),
        show_now=bool(result.showNow),
        hide_now=bool(result.hideNow),
        show_at_start=bool(result.showAtStart),
        hide_at_end=bool(result.hideAtEnd),
    )


def _presence_state(scene: _ir.Scene, obj: _ir.Object, time: float) -> tuple[bool, bool, bool]:
    tracks = scene._presence_tracks(obj)
    has_future = bool(tracks and tracks[-1]["timing"]["start_time"] > time)
    return bool(tracks), scene._presence_at(obj, time), has_future


def _resolve_ir(
    scene: _ir.Scene,
    obj: _ir.Object,
    intent: str,
    time: float,
    label: str,
) -> LifecyclePlan:
    has_tracks, present, has_future = _presence_state(scene, obj, time)
    return _resolve(
        intent,
        binding="this_scene",
        has_presence_timeline=has_tracks,
        present=present,
        has_future_event=has_future,
        at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
        label=label,
    )


def _resolve_wrapper(
    scene: _compat.Scene,
    member: _base.Mobject,
    intent: str,
    time: float,
    label: str,
) -> LifecyclePlan:
    if member._scene is None:
        return _resolve(
            intent,
            binding="detached",
            has_presence_timeline=False,
            present=True,
            has_future_event=False,
            at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
            label=label,
        )
    if member._scene is not scene:
        return _resolve(
            intent,
            binding="other_scene",
            has_presence_timeline=False,
            present=True,
            has_future_event=False,
            at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
            label=label,
        )
    assert member._object is not None
    has_tracks, present, has_future = _presence_state(scene, member._object, time)
    return _resolve(
        intent,
        binding="this_scene",
        has_presence_timeline=has_tracks,
        present=present,
        has_future_event=has_future,
        at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
        label=label,
    )


def _scene_add(
    self: _compat.Scene,
    *mobjects: object,
    key: str | None = None,
) -> _base.Mobject | _compat.Scene:
    if not mobjects:
        return self
    leaves = [member for value in mobjects for member in _compat._leaf_mobjects(value)]
    if key is not None and len(leaves) != 1:
        raise ValueError("an explicit key can only be used when adding one Mobject")

    for index, member in enumerate(leaves):
        plan = _resolve_wrapper(self, member, "add", self._cursor, "Scene.add target")
        if plan.bind:
            _phase_b._bind_raw(self, member, key=key if index == 0 else None)
        assert member._object is not None
        if plan.show_now:
            self._add_presence_track(
                member._object,
                False,
                True,
                self._cursor,
                key=f"@scene-add:{member._object.id}:{self._cursor:g}",
            )

    for value in mobjects:
        self._register_top_level(value)
    return leaves[0] if len(leaves) == 1 else self


def _scene_remove(self: _compat.Scene, *mobjects: object) -> _compat.Scene:
    leaves = [member for value in mobjects for member in _compat._leaf_mobjects(value)]
    for member in leaves:
        plan = _resolve_wrapper(self, member, "remove", self._cursor, "Scene.remove target")
        if plan.hide_now:
            assert member._object is not None
            self._add_presence_track(
                member._object,
                True,
                False,
                self._cursor,
                key=f"@scene-remove:{member._object.id}:{self._cursor:g}",
            )
    identities = {id(value) for value in mobjects}
    self._compat_top_level = [
        value for value in self._compat_top_level if id(value) not in identities
    ]
    return self


def _bind_introducer_target(self: _compat.Scene, target: object) -> None:
    leaves = _compat._leaf_mobjects(target)
    for member in leaves:
        plan = _resolve_wrapper(
            self,
            member,
            "introduce",
            self._cursor,
            "introducer target",
        )
        if plan.bind:
            _phase_b._bind_raw(self, member)
    self._register_top_level(target)


def _bind_for_animation(
    scene: _compat.Scene,
    value: object,
    *,
    start_time: float,
) -> None:
    for member in _compat._leaf_mobjects(value):
        plan = _resolve_wrapper(
            scene,
            member,
            "add",
            start_time,
            "animated Mobject",
        )
        if plan.bind:
            _phase_b._bind_raw(scene, member)
        assert member._object is not None
        if plan.show_now:
            scene._add_presence_track(
                member._object,
                False,
                True,
                start_time,
                key=f"@scene-play-add:{member._object.id}:{start_time:g}",
            )
    scene._register_top_level(value)


def _ensure_lifecycle_source_present(
    self: _ir.Scene,
    source: _ir.Object,
    start: float,
    label: str,
) -> None:
    _resolve_ir(self, source, "require_present", start, label)


def _ensure_lifecycle_target_available(
    self: _ir.Scene,
    target: _ir.Object,
    start: float,
    label: str,
) -> None:
    _resolve_ir(self, target, "require_available_target", start, f"{label} target")


def _add_presence_track(
    self: _ir.Scene,
    obj: _ir.Object,
    from_: bool,
    to: bool,
    time: float,
    *,
    key: str | None = None,
) -> None:
    existing = self._presence_tracks(obj)
    previous = existing[-1] if existing else None
    result = _validate_shared_presence_transition(
        previous is not None,
        0.0 if previous is None else float(previous["timing"]["start_time"]),
        False if previous is None else bool(previous["values"]["bool"]["to"]),
        float(time),
        bool(from_),
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))
    self._add_track(
        obj,
        "presence",
        {"bool": {"from": bool(from_), "to": bool(to)}},
        time,
        0.0,
        "linear",
        key,
    )


def _schedule_fade(
    self: _ir.Scene,
    obj: _ir.Object,
    *,
    fade_in: bool,
    key: str | None,
    duration: float,
    start_time: float,
    easing: str,
) -> None:
    if not isinstance(obj, _ir.Object) or obj._owner is not self._owner:
        raise ValueError("faded object must belong to this Scene")
    start = _ir._finite_number("start_time", start_time)
    run_duration = _ir._positive_number("duration", duration)
    end = start + run_duration
    previous_end = self._scheduled_fade_ends.get(obj.id)
    if previous_end is not None and start < previous_end:
        raise ValueError("fade animations for one object must not overlap")

    intent = "introduce" if fade_in else "remove_after_animation"
    plan = _resolve_ir(self, obj, intent, start, "fade target")
    tracks = self._presence_tracks(obj)
    object_key = self._object_keys[obj.id]
    direction = "in" if fade_in else "out"
    root_key = _ir._authoring_key(
        "key", key, f"@fade-{direction}:{object_key}:{start:g}"
    )
    from_ = self._appearance_at(obj, start)
    to = 1.0 if fade_in else 0.0

    if plan.show_at_start:
        self._add_presence_track(obj, False, True, start, key=f"{root_key}.show")

    self._add_scalar_track(
        obj,
        "appearance",
        _ir._unit_interval(
            "appearance from", from_ if tracks else (0.0 if fade_in else from_)
        ),
        to,
        start,
        run_duration,
        easing,
        root_key,
    )

    if plan.hide_at_end:
        self._add_presence_track(obj, True, False, end, key=f"{root_key}.hide")
    self._scheduled_fade_ends[obj.id] = end


def _schedule_create(
    self: _base.Scene,
    animation: _base.Create,
    *,
    duration: float,
    start_time: float,
    easing: str,
) -> None:
    obj = self._raw_object(animation.target)
    start = _ir._finite_number("start_time", start_time)
    run_duration = _ir._positive_number("duration", duration)
    end = start + run_duration

    snapshot = self._snapshot_for_object_at(obj, start)
    geometry = snapshot["geometry"]
    if not any(name in geometry for name in ("circle", "rectangle", "line", "vector_path")):
        raise ValueError("Create supports Circle, Rectangle/Square, Line, and VectorPath")

    plan = _resolve_ir(self, obj, "introduce", start, "Create target")
    for track in self._tracks:
        if track["object"] != obj.id or track["property"] != "reveal":
            continue
        track_start = track["timing"]["start_time"]
        track_end = track_start + track["timing"]["duration"]
        if track_start < end and start < track_end:
            raise ValueError("Create/reveal animations for one object must not overlap")

    object_key = self._object_keys[obj.id]
    root_key = animation.key or f"@create:{object_key}:{start:g}"
    if plan.show_at_start:
        self._add_presence_track(obj, False, True, start, key=f"{root_key}.show")
    self._add_scalar_track(
        obj,
        "reveal",
        0.0,
        1.0,
        start,
        run_duration,
        easing,
        root_key,
    )
    if self._appearance_at(obj, start) != 1.0:
        self._add_scalar_track(
            obj,
            "appearance",
            1.0,
            1.0,
            start,
            run_duration,
            "linear",
            f"{root_key}.appearance",
        )


def _int_func_mode(value: object) -> str:
    name = getattr(value, "__name__", None)
    if value is math.floor or name == "floor":
        return "floor"
    if value is math.ceil or name == "ceil":
        return "ceil"
    raise NotImplementedError(
        "subset-display animations currently support only floor/ceil int_func semantics"
    )


def _direct_members(group: object) -> list[_base.Mobject]:
    if not isinstance(group, _compat.Group):
        raise TypeError("subset-display animation requires a Group or VGroup")
    if not group.submobjects:
        raise ValueError("subset-display animation requires at least one direct submobject")
    members: list[_base.Mobject] = []
    for member in group.submobjects:
        if isinstance(member, _compat.Group) or not isinstance(member, _base.Mobject):
            raise NotImplementedError(
                "nested retained families are not yet supported by subset-display parity"
            )
        if not math.isclose(
            float(member._current_raw().style.get("opacity", 1.0)),
            1.0,
            abs_tol=1e-12,
        ):
            raise NotImplementedError(
                "Noon's low-level global opacity extension is not part of this Manim parity slice"
            )
        members.append(member)
    return members


def _snapshot_with_manim_opacity(member: _base.Mobject, opacity: float) -> dict[str, Any]:
    target = member.copy()
    target.set_opacity(opacity)
    return target.to_ir()


class ShowIncreasingSubsets:
    """ManimCE v0.21 direct-leaf subset display with exact discrete thresholds."""

    def __init__(
        self,
        group: object,
        suspend_mobject_updating: bool = False,
        int_func: object = math.floor,
        reverse_rate_function: bool = False,
        **kwargs: Any,
    ) -> None:
        if suspend_mobject_updating:
            raise NotImplementedError(
                "suspend_mobject_updating=True is not yet supported for subset-display parity"
            )
        if reverse_rate_function:
            raise NotImplementedError(
                "reverse_rate_function=True is not yet supported for subset-display parity"
            )
        self.mode = _int_func_mode(int_func)
        self.group = group
        self.mobject = group
        self.all_submobs = _direct_members(group)
        self.anim_args = dict(kwargs)
        self.visible_snapshots = [
            _snapshot_with_manim_opacity(member, 1.0) for member in self.all_submobs
        ]
        for member in self.all_submobs:
            # Pinned ManimCE constructor semantics: set_opacity(0) overwrites fill
            # and stroke alpha on the actual supplied direct submobject. This remains
            # valid after Scene.add: Manim mutates the same already-bound mobjects.
            member.set_opacity(0.0)
        self.hidden_snapshots = [member.to_ir() for member in self.all_submobs]


class ShowSubmobjectsOneByOne(ShowIncreasingSubsets):
    """Show exactly one direct child at a time with Manim's default ceil semantics."""

    def __init__(
        self,
        group: object,
        int_func: object = math.ceil,
        **kwargs: Any,
    ) -> None:
        try:
            members = list(group)  # type: ignore[arg-type]
        except TypeError as error:
            raise TypeError("ShowSubmobjectsOneByOne group must be iterable") from error
        new_group = _compat.Group(*members)
        super().__init__(new_group, int_func=int_func, **kwargs)


def _inverse_monotonic_rate(rate_id: str, target: float) -> float:
    value = float(target)
    if value <= 0.0:
        return 0.0
    if value >= 1.0:
        return 1.0
    if rate_id == "there_and_back":
        raise NotImplementedError(
            "subset-display parity currently requires a monotonic rate function"
        )
    if rate_id not in {
        "linear",
        "smooth",
        "rush_into",
        "rush_from",
        "ease_in_out_cubic",
    }:
        raise NotImplementedError(
            f"subset-display parity does not yet support rate function {rate_id}"
        )
    # Preserve exact fixed points such as smooth(0.5) == 0.5. Boundary equality is
    # observable for floor/ceil subset selection, so do not perturb it through a
    # numerical inverse when the target itself is already the exact inverse.
    if math.isclose(
        _rate_functions.evaluate_rate_function(rate_id, value),
        value,
        rel_tol=0.0,
        abs_tol=1e-15,
    ):
        return value
    low = 0.0
    high = 1.0
    for _ in range(80):
        middle = (low + high) * 0.5
        if _rate_functions.evaluate_rate_function(rate_id, middle) < value:
            low = middle
        else:
            high = middle
    return (low + high) * 0.5


def _add_subset_transform_track(
    scene: _compat.Scene,
    obj: _ir.Object,
    from_snapshot: dict[str, Any],
    to_snapshot: dict[str, Any],
    *,
    start_time: float,
    duration: float,
    easing: str,
    key: str,
) -> None:
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
        key,
    )


def _schedule_subset_display(
    scene: _compat.Scene,
    animation: ShowIncreasingSubsets,
    *,
    start_time: float,
    duration: float,
    rate_id: str,
) -> None:
    start = _ir._finite_number("start_time", start_time)
    run_time = _ir._positive_number("duration", duration)
    end = start + run_time
    _bind_for_animation(scene, animation.mobject, start_time=start)
    count = len(animation.all_submobs)

    if animation.mode == "floor":
        for index, member in enumerate(animation.all_submobs):
            assert member._object is not None
            threshold = start + run_time * _inverse_monotonic_rate(
                rate_id, (index + 1) / count
            )
            _add_subset_transform_track(
                scene,
                member._object,
                animation.hidden_snapshots[index],
                animation.visible_snapshots[index],
                start_time=start,
                duration=threshold - start,
                easing="step_end",
                key=f"@show-increasing:{member._object.id}:{start:g}",
            )
            scene._scheduled_transform_targets[member._object.id] = copy.deepcopy(
                animation.visible_snapshots[index]
            )
            scene._scheduled_transform_ends[member._object.id] = end
        return

    # Ceil semantics are used by ShowSubmobjectsOneByOne. At each exact k/N
    # threshold the previous child remains visible; the new child appears only for
    # progress strictly greater than that threshold. step_start preserves precisely
    # that left-open transition without epsilon-duration approximations.
    for index, member in enumerate(animation.all_submobs):
        assert member._object is not None
        show_start = start + run_time * _inverse_monotonic_rate(rate_id, index / count)
        show_end = start + run_time * _inverse_monotonic_rate(
            rate_id, (index + 1) / count
        )
        _add_subset_transform_track(
            scene,
            member._object,
            animation.hidden_snapshots[index],
            animation.visible_snapshots[index],
            start_time=show_start,
            duration=show_end - show_start,
            easing="step_start",
            key=f"@show-one:{member._object.id}:{start:g}.show",
        )
        final_snapshot = animation.visible_snapshots[index]
        if index + 1 < count:
            _add_subset_transform_track(
                scene,
                member._object,
                animation.visible_snapshots[index],
                animation.hidden_snapshots[index],
                start_time=show_end,
                duration=end - show_end,
                easing="step_start",
                key=f"@show-one:{member._object.id}:{start:g}.hide",
            )
            final_snapshot = animation.hidden_snapshots[index]
        scene._scheduled_transform_targets[member._object.id] = copy.deepcopy(final_snapshot)
        scene._scheduled_transform_ends[member._object.id] = end


def _resolve_subset_options(
    animation: ShowIncreasingSubsets,
    *,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
):
    resolved = _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=0.0,
        play_run_time=play_run_time,
        play_easing=play_easing,
        play_rate_func=play_rate_func,
        play_lag_ratio=play_lag_ratio,
    )
    _inverse_monotonic_rate(resolved.rate_func, 0.5)
    return resolved


def _subset_scene_play(
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
    subsets = [
        animation for animation in animations if isinstance(animation, ShowIncreasingSubsets)
    ]
    if not subsets:
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
    if len(subsets) != len(animations):
        raise NotImplementedError(
            "mixing top-level subset-display animations with unrelated animations remains partial; use AnimationGroup"
        )
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
    for animation in subsets:
        for member in animation.all_submobs:
            _animate._record_wrapper_state(member, wrapper_states)

    max_end = base_start
    try:
        for animation in subsets:
            resolved = _resolve_subset_options(
                animation,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule_subset_display(
                self,
                animation,
                start_time=base_start,
                duration=resolved.run_time,
                rate_id=resolved.rate_func,
            )
            max_end = max(max_end, base_start + resolved.run_time)
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


def _record_composition_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    if isinstance(animation, ShowIncreasingSubsets):
        for member in animation.all_submobs:
            _animate._record_wrapper_state(member, states)
        return
    _ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE(animation, states)


def _composition_play_leaf(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
    time_map_steps: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]],
) -> None:
    if not isinstance(animation, ShowIncreasingSubsets):
        _ORIGINAL_COMPOSITION_PLAY_LEAF(
            scene,
            animation,
            start_time=start_time,
            run_time=run_time,
            time_map_steps=time_map_steps,
            pending_time_maps=pending_time_maps,
        )
        return

    resolved = _resolve_subset_options(
        animation,
        play_run_time=run_time,
        play_easing=None,
        play_rate_func=None,
        play_lag_ratio=None,
    )
    track_start = len(scene._tracks)
    _schedule_subset_display(
        scene,
        animation,
        start_time=start_time,
        duration=resolved.run_time,
        rate_id=resolved.rate_func,
    )
    track_end = len(scene._tracks)
    if track_end <= track_start or not _composition._path_requires_time_map(time_map_steps):
        return

    # Subset-display leaves consist of several local step intervals. Preserve each
    # interval before applying a nonlinear parent AnimationGroup/LaggedStart time map.
    for index in range(track_start, track_end):
        track = scene._tracks[index]
        local_start = (float(track["timing"]["start_time"]) - start_time) / resolved.run_time
        local_duration = float(track["timing"]["duration"]) / resolved.run_time
        steps = list(time_map_steps)
        if not (
            math.isclose(local_start, 0.0, abs_tol=1e-12)
            and math.isclose(local_duration, 1.0, abs_tol=1e-12)
        ):
            steps.append(
                {
                    "start": local_start,
                    "duration": local_duration,
                    "rate_func": "linear",
                }
            )
        pending_time_maps.append((index, index + 1, steps))


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _compat.Scene.add = _scene_add
    _compat.Scene.remove = _scene_remove
    _compat.Scene._bind_introducer_target = _bind_introducer_target
    _animate._bind_for_animation = _bind_for_animation

    _ir.Scene._ensure_lifecycle_source_present = _ensure_lifecycle_source_present
    _ir.Scene._ensure_lifecycle_target_available = _ensure_lifecycle_target_available
    _ir.Scene._add_presence_track = _add_presence_track
    _ir.Scene._schedule_fade = _schedule_fade
    _base.Scene._schedule_create = _schedule_create

    public = {
        "ShowIncreasingSubsets": ShowIncreasingSubsets,
        "ShowSubmobjectsOneByOne": ShowSubmobjectsOneByOne,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        setattr(_animate, name, value)
    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports

    _compat.Scene.play = _subset_scene_play
    _composition._record_composition_wrapper_state = _record_composition_wrapper_state
    _composition._play_leaf = _composition_play_leaf


install()
