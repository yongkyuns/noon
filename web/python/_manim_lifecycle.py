"""Thin Python adapter for Noon's shared Rust lifecycle authoring semantics.

Python owns wrapper identity and emits the canonical scene operations requested by
Rust. Presence legality, reintroduction/removal rules, source/target requirements,
and presence-chain validation are resolved by the shared core planner.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

from js import noonResolveLifecyclePlan as _resolve_shared_lifecycle
from js import noonValidatePresenceTransition as _validate_shared_presence_transition

import noon as _base
import _noon_ir as _ir
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_phase_b as _phase_b

_INSTALLED = False


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


install()
