"""Canonical retained family Create/Uncreate authoring.

This is a thin Manim-compatible syntax layer over the Rust-owned semantic-family
animation request builder. Python reports already-bound leaf ObjectIds in its normal
wrapper traversal; Rust validates those leaves against authoritative SemanticStore order
and serializes one glyph/resource-free FamilyAnimationRequest into canonical SceneSpec.
"""

from __future__ import annotations

import copy
import json
import math
from typing import Any

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_lifecycle as _lifecycle
import _manim_phase_b as _phase_b
import _manim_retained_animate as _retained
import _manim_semantic_handles as _semantic_handles
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = None
_ORIGINAL_RETAINED_DOCUMENT = None


def _native_text(value: object) -> bool:
    return isinstance(value, _typst.Text)


def _family_candidate(animation: object):
    if not isinstance(animation, (_animate.Create, _animate.Uncreate)):
        return None
    target = animation.target
    leaves = _compat._leaf_mobjects(target)
    if not leaves or not any(_native_text(member) for member in leaves):
        return None

    for member in leaves:
        if isinstance(member, _typst._RetainedTextMobject) and not _native_text(member):
            raise NotImplementedError(
                "canonical family Create/Uncreate currently supports native Text; "
                "Typst and MathTypst need their retained family identity bound to the same contract"
            )
        if not isinstance(member, _base.Mobject):
            raise TypeError("family Create/Uncreate target contains a non-Mobject leaf")

    if isinstance(target, _compat.Group):
        family = target
        synthetic = False
    else:
        # A single native Text is still a Manim family of rendered members. Build an
        # authoring-only one-member semantic family; it is never inserted into scene
        # membership or serialized as a renderer object.
        family = _compat.Group(target)
        synthetic = True
    family_handle = getattr(family, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "familyAnimationRequest"):
        raise RuntimeError("family Create/Uncreate requires the shared Rust authoring family handle")
    return target, family, leaves, synthetic


def _geometry_lifecycle(
    scene: _compat.Scene,
    member: _base.Mobject,
    intent: str,
    start_time: float,
    label: str,
):
    plan = _lifecycle._resolve_wrapper(scene, member, intent, start_time, label)
    if plan.bind:
        _phase_b._bind_raw(scene, member)
    if member._object is None:
        raise RuntimeError("family animation geometry leaf did not bind to a scene object")
    return plan


def _text_lifecycle(
    scene: _compat.Scene,
    member: _typst.Text,
    intent: str,
    start_time: float,
    label: str,
):
    plan = _retained._resolve_retained_lifecycle(scene, member, intent, start_time, label)
    if plan.bind:
        _retained._bind_retained(scene, member)
    if member._retained_object_id is None:
        raise RuntimeError("family animation Text leaf did not bind to a retained scene object")
    _retained._ensure_animation_state(scene)
    scene._retained_animation_state.setdefault(
        int(member.id), _retained._initial_animation_state(member)
    )
    return plan


def _show_at_start(
    scene: _compat.Scene,
    member: _base.Mobject,
    plan: object,
    start_time: float,
) -> None:
    if not bool(getattr(plan, "show_at_start", False)):
        return
    if _native_text(member):
        state = scene._retained_animation_state[int(member.id)]
        state["presence"] = False
        _retained._append_presence_track(
            scene,
            object_id=int(member.id),
            current=False,
            target=True,
            start_time=start_time,
        )
        state["presence"] = True
        return
    assert member._object is not None
    scene._add_presence_track(
        member._object,
        False,
        True,
        start_time,
        key=f"@family-create:{member._object.id}:{start_time:g}.show",
    )


def _hide_at_end(
    scene: _compat.Scene,
    member: _base.Mobject,
    plan: object,
    end_time: float,
) -> None:
    if not bool(getattr(plan, "hide_at_end", False)):
        return
    if _native_text(member):
        state = scene._retained_animation_state[int(member.id)]
        _retained._append_presence_track(
            scene,
            object_id=int(member.id),
            current=True,
            target=False,
            start_time=end_time,
        )
        state["presence"] = False
        return
    assert member._object is not None
    scene._add_presence_track(
        member._object,
        True,
        False,
        end_time,
        key=f"@family-uncreate:{member._object.id}:{end_time:g}.hide",
    )


def _bind_family(
    scene: _compat.Scene,
    animation: object,
    target: object,
    leaves: list[_base.Mobject],
    *,
    start_time: float,
    duration: float,
) -> None:
    uncreate = isinstance(animation, _animate.Uncreate)
    end_time = start_time + duration
    lifecycle_plans: list[tuple[_base.Mobject, object]] = []

    for member in leaves:
        if uncreate:
            # Match Scene.play's implicit-add behavior first, then apply the remover
            # lifecycle only when this Uncreate instance owns cleanup.
            if _native_text(member):
                add_plan = _text_lifecycle(
                    scene, member, "add", start_time, "animated Uncreate target"
                )
            else:
                add_plan = _geometry_lifecycle(
                    scene, member, "add", start_time, "animated Uncreate target"
                )
            if bool(getattr(add_plan, "show_now", False)):
                _show_at_start(scene, member, type("Plan", (), {"show_at_start": True})(), start_time)
            intent = "remove_after_animation" if bool(animation.remover) else "require_present"
        else:
            intent = "introduce"

        plan = (
            _text_lifecycle(scene, member, intent, start_time, "family animation target")
            if _native_text(member)
            else _geometry_lifecycle(scene, member, intent, start_time, "family animation target")
        )
        lifecycle_plans.append((member, plan))
        if not uncreate:
            _show_at_start(scene, member, plan, start_time)

    # Scene membership is represented by the source-level family wrapper, not by the
    # flattened leaves used for runtime materialization.
    leaf_ids = {id(member) for member in leaves}
    scene._compat_top_level = [
        value for value in scene._compat_top_level if id(value) not in leaf_ids
    ]
    scene._register_top_level(target)

    if uncreate and bool(animation.remover):
        for member, plan in lifecycle_plans:
            _hide_at_end(scene, member, plan, end_time)
        scene._compat_top_level = [
            value for value in scene._compat_top_level if value is not target
        ]


def _append_request(
    scene: _compat.Scene,
    animation: object,
    family: _compat.Group,
    leaves: list[_base.Mobject],
    *,
    start_time: float,
    duration: float,
    lag_ratio: float,
    rate_function: str,
) -> None:
    requests = getattr(scene, "_retained_family_animations", None)
    if requests is None:
        requests = []
        scene._retained_family_animations = requests
    if requests:
        raise NotImplementedError(
            "canonical retained execution currently supports one family animation request per scene"
        )

    reverse_rate = bool(animation.reverse_rate_function) if isinstance(
        animation, _animate.Uncreate
    ) else False
    session = family._semantic_family_handle.familyAnimationRequest(
        "reveal",
        float(start_time),
        float(duration),
        float(lag_ratio),
        str(rate_function),
        reverse_rate,
        False,
    )
    for member in leaves:
        if _native_text(member):
            identity = member._semantic_family_member_handle
            if identity is None:
                raise RuntimeError("native Text has no shared semantic family identity")
            session.bindRetainedNativeText(
                identity,
                member._retained_handle,
                float(member.id),
            )
            continue
        handle = _semantic_handles._handle_for(member)
        if handle is None:
            raise RuntimeError(
                "family animation geometry leaf has no current shared semantic handle"
            )
        session.bindMobject(handle, float(member.id))

    requests.append(json.loads(str(session.finishJson())))


def _family_scene_play(
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
    candidates = [_family_candidate(animation) for animation in animations]
    if not any(candidate is not None for candidate in candidates):
        assert _ORIGINAL_SCENE_PLAY is not None
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
    if len(animations) != 1 or candidates[0] is None:
        raise NotImplementedError(
            "canonical family Create/Uncreate must currently be the only animation in Scene.play"
        )
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

    animation = animations[0]
    target, family, leaves, _synthetic = candidates[0]
    play_run_time = run_time if run_time is not None else duration
    if play_run_time is not None:
        play_run_time = float(play_run_time)
    play_lag_ratio = None if lag_ratio is None else float(lag_ratio)
    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")

    resolved = _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=1.0,
        play_run_time=play_run_time,
        play_easing=easing,
        play_rate_func=rate_func,
        play_lag_ratio=play_lag_ratio,
    )
    if not math.isclose(resolved.path_arc, 0.0, abs_tol=1e-15):
        raise NotImplementedError("family Create/Uncreate does not support path_arc")

    checkpoint = self._authoring_checkpoint()
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    retained_objects_before = list(getattr(self, "_retained_text_objects", []))
    next_object_id_before = getattr(self, "_retained_next_object_id", None)
    next_order_before = getattr(self, "_retained_next_painter_order", None)
    retained_tracks_before = copy.deepcopy(getattr(self, "_retained_animation_tracks", []))
    retained_state_before = copy.deepcopy(getattr(self, "_retained_animation_state", {}))
    family_requests_before = copy.deepcopy(getattr(self, "_retained_family_animations", []))

    geometry_states: dict[int, tuple[_base.Mobject, object, object]] = {}
    retained_states: dict[int, tuple[_typst._RetainedTextMobject, object, object, object]] = {}
    for member in leaves:
        if _native_text(member):
            retained_states[id(member)] = (
                member,
                member._scene,
                member._retained_object_id,
                member._retained_order,
            )
        else:
            _animate._record_wrapper_state(member, geometry_states)

    try:
        _bind_family(
            self,
            animation,
            target,
            leaves,
            start_time=base_start,
            duration=resolved.run_time,
        )
        _append_request(
            self,
            animation,
            family,
            leaves,
            start_time=base_start,
            duration=resolved.run_time,
            lag_ratio=resolved.lag_ratio,
            rate_function=resolved.rate_func,
        )
        self._cursor = max(cursor_before, base_start + resolved.run_time)
        return self
    except Exception:
        self._restore_authoring_checkpoint(checkpoint)
        self._cursor = cursor_before
        self._compat_top_level = top_level_before
        self._retained_text_objects = retained_objects_before
        if next_object_id_before is not None:
            self._retained_next_object_id = next_object_id_before
        if next_order_before is not None:
            self._retained_next_painter_order = next_order_before
        self._retained_animation_tracks = retained_tracks_before
        self._retained_animation_state = retained_state_before
        self._retained_family_animations = family_requests_before
        for member, old_scene, old_object in geometry_states.values():
            member._scene = old_scene
            member._object = old_object
        for member, old_scene, old_object_id, old_order in retained_states.values():
            member._scene = old_scene
            member._retained_object_id = old_object_id
            member._retained_order = old_order
        raise


def _retained_document(self: _compat.Scene) -> dict[str, Any]:
    assert _ORIGINAL_RETAINED_DOCUMENT is not None
    document = _ORIGINAL_RETAINED_DOCUMENT(self)
    requests = getattr(self, "_retained_family_animations", None)
    if requests:
        document["family_animations"] = copy.deepcopy(requests)
    return document


def install() -> None:
    """Install after the ordinary retained and DrawBorderThenFill Scene.play wrappers."""

    global _INSTALLED, _ORIGINAL_SCENE_PLAY, _ORIGINAL_RETAINED_DOCUMENT
    if _INSTALLED:
        return
    _INSTALLED = True
    _ORIGINAL_SCENE_PLAY = _compat.Scene.play
    _ORIGINAL_RETAINED_DOCUMENT = _compat.Scene.retained_document
    _compat.Scene.play = _family_scene_play
    _compat.Scene.retained_document = _retained_document
