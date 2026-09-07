"""Thin Python adapter for Noon's shared Rust lifecycle authoring semantics.

Python owns wrapper identity and emits the canonical scene operations requested by
Rust. Presence legality, reintroduction/removal rules, source/target requirements,
and presence-chain validation are resolved by the shared core planner.

Subset-display classes in this module retain only Manim-specific constructor
ergonomics. Shared Rust validates and prepares the family, resolves thresholds,
and owns playback and endpoint reconciliation.
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
    has_tracks, present, has_future = member._scene_lifecycle_state(scene, time)
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
            member._record_scene_presence(
                self,
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
            member._record_scene_presence(
                self,
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
            member._record_scene_presence(
                scene,
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


def _subset_family(group: object) -> tuple[_compat.Group, list[_base.Mobject], object]:
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
        members.append(member)
    family = getattr(group, "_semantic_family_handle", None)
    if family is None:
        raise NotImplementedError(
            "subset-display animation requires an ordinary typed family"
        )
    return group, members, family


def _prepare_subset_family(group: object) -> tuple[_compat.Group, list[_base.Mobject]]:
    typed_group, members, family = _subset_family(group)
    bound_scenes = {member._scene for member in members if member._scene is not None}
    if len(bound_scenes) > 1:
        raise ValueError("subset-display family members belong to different Scenes")
    try:
        # Live-created members remain detached until the subset animation admits
        # them, but their semantic mutations still belong to the active session.
        # Use the same family context resolution as shared arrangement/targets.
        import _manim_semantic_handles as _semantic_handles

        context = _semantic_handles._group_target_context(typed_group)
        if context is not None:
            context.prepareFamilySubsetDisplay(family)
        elif bound_scenes:
            scene = next(iter(bound_scenes))
            context = getattr(scene, "_canonical_authoring_context", None)
            if context is None:
                raise NotImplementedError(
                    "already-bound subset-display families require canonical shared execution"
                )
            context.prepareFamilySubsetDisplay(family)
        else:
            family.prepareSubsetDisplay()
    except Exception as error:
        raise ValueError(str(error)) from None
    return typed_group, members


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
        if _int_func_mode(int_func) != "floor":
            raise NotImplementedError(
                "ShowIncreasingSubsets currently supports only its floor int_func"
            )
        self.mode = "increasing-floor"
        self.group, _ = _prepare_subset_family(group)
        self.mobject = self.group
        self.anim_args = dict(kwargs)


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
        if _int_func_mode(int_func) != "ceil":
            raise NotImplementedError(
                "ShowSubmobjectsOneByOne currently supports only its ceil int_func"
            )
        self.mode = "one-by-one-ceil"
        self.group, _ = _prepare_subset_family(_compat.Group(*members))
        self.mobject = self.group
        self.anim_args = dict(kwargs)


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

install()
