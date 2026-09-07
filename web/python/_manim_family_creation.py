"""Canonical retained family creation-animation authoring.

This is a thin Manim-compatible syntax/lifecycle layer over Rust-owned semantic-family
animation request builders. Python reports already-bound leaf ObjectIds in its normal
wrapper traversal; Rust validates authoritative SemanticStore order, derives retained
Text animation-member cardinality, and serializes glyph/resource-free requests into
canonical SceneSpec.
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
import _manim_rate_functions as _rate_functions
import _manim_retained_animate as _retained
import _manim_semantic_handles as _semantic_handles
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = None
_ORIGINAL_RETAINED_DOCUMENT = None


def _native_text(value: object) -> bool:
    return isinstance(value, _typst.Text)


class Write:
    """Simulate writing retained native Text with ManimCE v0.21 family semantics."""

    def __init__(
        self,
        vmobject: object,
        rate_func: object = _rate_functions.linear,
        reverse: bool = False,
        **kwargs: Any,
    ) -> None:
        if not isinstance(vmobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Write target must be a Mobject or Group")

        animation_kwargs = dict(kwargs)
        if "introducer" in animation_kwargs:
            raise TypeError("Write owns introducer from its reverse option")
        stroke_width = float(animation_kwargs.pop("stroke_width", 2.0))
        stroke_color = animation_kwargs.pop("stroke_color", None)
        if not math.isfinite(stroke_width) or stroke_width < 0.0:
            raise ValueError("Write stroke_width must be finite and non-negative")
        leaves = _compat._leaf_mobjects(vmobject)
        retained_text = any(_native_text(member) for member in leaves)
        if retained_text and (
            not math.isclose(stroke_width, 2.0, abs_tol=1e-15) or stroke_color is not None
        ):
            raise NotImplementedError(
                "retained Text Write currently supports Manim's default outline style only"
            )

        remover = animation_kwargs.pop("remover", bool(reverse))
        self.mobject = vmobject
        self.target = vmobject
        self.reverse = bool(reverse)
        self.introducer = not self.reverse
        self.remover = bool(remover)
        self.reverse_rate_function = bool(
            animation_kwargs.get("reverse_rate_function", False)
        )
        self.stroke_width = stroke_width
        self.stroke_color = (
            None
            if stroke_color is None
            else _phase_b._as_color("stroke_color", stroke_color)
        )
        animation_kwargs["rate_func"] = rate_func
        self.anim_args = animation_kwargs


class Unwrite(Write):
    """Simulate erasing retained native Text with ManimCE v0.21 family semantics."""

    def __init__(
        self,
        vmobject: object,
        rate_func: object = _rate_functions.linear,
        reverse: bool = True,
        **kwargs: Any,
    ) -> None:
        if "reverse_rate_function" in kwargs:
            raise TypeError("Unwrite owns reverse_rate_function=True")
        animation_kwargs = dict(kwargs)
        animation_kwargs["reverse_rate_function"] = True
        super().__init__(
            vmobject,
            rate_func=rate_func,
            reverse=reverse,
            **animation_kwargs,
        )


def _is_reveal_animation(animation: object) -> bool:
    return isinstance(animation, (_animate.Create, _animate.Uncreate))


def _is_write_animation(animation: object) -> bool:
    return isinstance(animation, Write)


def _family_candidate(animation: object):
    if not (_is_reveal_animation(animation) or _is_write_animation(animation)):
        return None

    target = animation.target
    leaves = _compat._leaf_mobjects(target)
    if not leaves:
        return None

    if _is_write_animation(animation):
        native_text = [_native_text(member) for member in leaves]
        if not any(native_text):
            if animation.reverse or animation.remover or animation.reverse_rate_function:
                raise NotImplementedError(
                    "reverse ordinary vector Write/Unwrite remains a retained compatibility case (#959)"
                )
            # Forward ordinary vector Write is classified by the final canonical
            # Scene.play layer as shared family DrawBorderThenFill.
            return None
        if not all(native_text):
            raise NotImplementedError(
                "Write cannot mix retained Text and ordinary vector leaves"
            )
    elif not any(_native_text(member) for member in leaves):
        return None

    label = "Write/Unwrite" if _is_write_animation(animation) else "Create/Uncreate"
    for member in leaves:
        if isinstance(member, _typst._RetainedTextMobject) and not _native_text(member):
            raise NotImplementedError(
                f"canonical family {label} currently supports native Text; "
                "Typst and MathTypst need their retained family identity bound to the same contract"
            )
        if not isinstance(member, _base.Mobject):
            raise TypeError(f"family {label} target contains a non-Mobject leaf")

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
    method = (
        "familyWriteAnimationRequest"
        if _is_write_animation(animation)
        else "familyAnimationRequest"
    )
    if family_handle is None or not hasattr(family_handle, method):
        raise RuntimeError(f"family {label} requires the shared Rust authoring family handle")
    return target, family, leaves, synthetic


def _value_contains_retained_text(value: object) -> bool:
    if isinstance(value, _typst._RetainedTextMobject):
        return True
    if isinstance(value, _compat.Group):
        return any(
            isinstance(member, _typst._RetainedTextMobject)
            for member in _compat._leaf_mobjects(value)
        )
    return False


def _ordinary_requires_retained_scheduler(animation: object) -> bool:
    """Return whether an ordinary animation touches retained Text content."""

    values = (
        _animate._builder_source(animation),
        getattr(animation, "source", None),
        getattr(animation, "target", None),
    )
    return any(_value_contains_retained_text(value) for value in values if value is not None)


def _retained_ordinary_plan(animation: object) -> list[dict[str, Any]] | None:
    """Classify one non-family animation without inventing a second retained scheduler."""

    if isinstance(animation, (_animate._AlignedGroupAnimationBuilder, _animate.Indicate)):
        raise NotImplementedError(
            "family Transform and Indicate require shared canonical composition playback"
        )

    plan = _retained._retained_animation_plan(animation)
    if plan is not None:
        return plan
    if _ordinary_requires_retained_scheduler(animation):
        raise NotImplementedError(
            "retained family animations can currently compose with direct retained Text "
            "property animations and fades; retained Group/VGroup property scheduling "
            "must first use the shared retained family scheduler"
        )
    return None


def _ordinary_scene_leaves(animation: object) -> list[_base.Mobject]:
    """Return source-level scene leaves an ordinary animation may bind or mutate."""

    roots: list[object] = []
    builder_source = _animate._builder_source(animation)
    if builder_source is not None:
        roots.append(builder_source)
    else:
        source = getattr(animation, "source", None)
        if source is not None:
            roots.append(source)
    if isinstance(animation, (_animate.Create, _animate.Uncreate, _animate.FadeIn, _animate.FadeOut)):
        roots.append(animation.target)
    elif isinstance(animation, (_animate.ReplacementTransform, _animate.TransformFromCopy)):
        roots.append(animation.target)

    result: list[_base.Mobject] = []
    seen: set[int] = set()
    for root in roots:
        if not isinstance(root, (_base.Mobject, _compat.Group)):
            continue
        for member in _compat._leaf_mobjects(root):
            if isinstance(member, _base.Mobject) and id(member) not in seen:
                seen.add(id(member))
                result.append(member)
    return result


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
        key=f"@family-remove:{member._object.id}:{end_time:g}.hide",
    )


def _animation_introducer(animation: object) -> bool:
    if isinstance(animation, _animate.Uncreate):
        return False
    if isinstance(animation, _animate.Create):
        return True
    if isinstance(animation, Write):
        return bool(animation.introducer)
    raise TypeError("unsupported family creation animation")


def _animation_remover(animation: object) -> bool:
    if isinstance(animation, _animate.Uncreate):
        return bool(animation.remover)
    if isinstance(animation, _animate.Create):
        return False
    if isinstance(animation, Write):
        return bool(animation.remover)
    raise TypeError("unsupported family creation animation")


def _begin_family_lifecycle(
    scene: _compat.Scene,
    animation: object,
    target: object,
    leaves: list[_base.Mobject],
    *,
    start_time: float,
) -> list[tuple[_base.Mobject, object]]:
    introducer = _animation_introducer(animation)
    remover = _animation_remover(animation)
    lifecycle_plans: list[tuple[_base.Mobject, object]] = []

    for member in leaves:
        if not introducer:
            # Match Scene.play's implicit-add behavior before applying removal or
            # require-present semantics. This covers Uncreate, reverse Write, and
            # default Unwrite from one lifecycle rule.
            add_plan = (
                _text_lifecycle(scene, member, "add", start_time, "animated family target")
                if _native_text(member)
                else _geometry_lifecycle(scene, member, "add", start_time, "animated family target")
            )
            if bool(getattr(add_plan, "show_now", False)):
                _show_at_start(
                    scene,
                    member,
                    type("Plan", (), {"show_at_start": True})(),
                    start_time,
                )
            intent = "remove_after_animation" if remover else "require_present"
        else:
            intent = "introduce"

        plan = (
            _text_lifecycle(scene, member, intent, start_time, "family animation target")
            if _native_text(member)
            else _geometry_lifecycle(scene, member, intent, start_time, "family animation target")
        )
        lifecycle_plans.append((member, plan))
        if introducer:
            _show_at_start(scene, member, plan, start_time)

    # Scene membership is represented by the source-level family wrapper, not by the
    # flattened leaves used for runtime materialization.
    leaf_ids = {id(member) for member in leaves}
    scene._compat_top_level = [
        value for value in scene._compat_top_level if id(value) not in leaf_ids
    ]
    scene._register_top_level(target)
    return lifecycle_plans


def _finish_family_lifecycle(
    scene: _compat.Scene,
    animation: object,
    target: object,
    lifecycle_plans: list[tuple[_base.Mobject, object]],
    *,
    end_time: float,
) -> None:
    if not _animation_remover(animation):
        return
    for member, plan in lifecycle_plans:
        _hide_at_end(scene, member, plan, end_time)
    scene._compat_top_level = [
        value for value in scene._compat_top_level if value is not target
    ]


def _request_list(scene: _compat.Scene) -> list[dict[str, Any]]:
    requests = getattr(scene, "_retained_family_animations", None)
    if requests is None:
        requests = []
        scene._retained_family_animations = requests
    # Preserve source play order. Rust owns plural-plan validation, including rejecting
    # time-overlapping ownership of the same retained object before playback starts.
    return requests


def _bind_request_leaves(session: object, leaves: list[_base.Mobject]) -> None:
    for member in leaves:
        if _native_text(member) and getattr(member, "_semantic_handle", None) is None:
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


def _append_reveal_request(
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
    requests = _request_list(scene)
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
    _bind_request_leaves(session, leaves)
    requests.append(json.loads(str(session.finishJson())))


def _write_request_inputs(
    animation: Write,
    *,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
) -> tuple[float | None, float | None, str, bool, bool]:
    args = _options.builder_args(animation)

    path_arc = float(args.get("path_arc", 0.0))
    if not math.isfinite(path_arc):
        raise ValueError("family Write path_arc must be finite")
    if not math.isclose(path_arc, 0.0, abs_tol=1e-15):
        raise NotImplementedError("family Write/Unwrite does not support path_arc")

    run_time_value = play_run_time if play_run_time is not None else args.get("run_time")
    run_time_override = None if run_time_value is None else float(run_time_value)
    lag_value = play_lag_ratio if play_lag_ratio is not None else args.get("lag_ratio")
    lag_override = None if lag_value is None else float(lag_value)

    if play_easing is not None:
        rate_id = str(play_easing)
    elif play_rate_func is not None:
        rate_id = _compat._easing_from_rate_func(play_rate_func)
    else:
        rate_id = _compat._easing_from_rate_func(
            args.get("rate_func", _rate_functions.linear)
        )

    reverse_rate = bool(
        args.get("reverse_rate_function", animation.reverse_rate_function)
    )
    reverse_members = bool(animation.reverse)
    return run_time_override, lag_override, str(rate_id), reverse_rate, reverse_members


def _append_write_request(
    scene: _compat.Scene,
    animation: Write,
    family: _compat.Group,
    leaves: list[_base.Mobject],
    *,
    start_time: float,
    duration_override: float | None,
    lag_ratio_override: float | None,
    rate_function: str,
    reverse_rate_function: bool,
    reverse_member_order: bool,
) -> tuple[float, float]:
    requests = _request_list(scene)
    session = family._semantic_family_handle.familyWriteAnimationRequest(
        float(start_time),
        duration_override,
        lag_ratio_override,
        str(rate_function),
        bool(reverse_rate_function),
        bool(reverse_member_order),
    )
    _bind_request_leaves(session, leaves)
    result = json.loads(str(session.finishJson()))
    request = result.get("request")
    if not isinstance(request, dict):
        raise RuntimeError("Rust family Write authoring returned no canonical request")
    run_time = float(result.get("run_time"))
    lag_ratio = float(result.get("lag_ratio"))
    if not math.isfinite(run_time) or run_time <= 0.0:
        raise RuntimeError("Rust family Write authoring returned invalid run_time")
    if not math.isfinite(lag_ratio) or lag_ratio < 0.0:
        raise RuntimeError("Rust family Write authoring returned invalid lag_ratio")
    requests.append(request)
    return run_time, lag_ratio


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
    family_count = sum(candidate is not None for candidate in candidates)
    if family_count == 0:
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
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

    # Resolve retained-property intent before any Scene mutation. Direct retained Text
    # animations reuse the existing retained scheduler; geometry remains on the aligned
    # scheduler. A retained Group/VGroup that has not joined the shared retained-family
    # scheduler still fails explicitly rather than falling through to legacy geometry.
    retained_ordinary_plans = [
        None if candidate is not None else _retained_ordinary_plan(animation)
        for animation, candidate in zip(animations, candidates, strict=True)
    ]

    leaf_owners: dict[int, int] = {}
    for animation_index, candidate in enumerate(candidates):
        if candidate is None:
            continue
        _target, _family, leaves, _synthetic = candidate
        for member in leaves:
            previous = leaf_owners.get(id(member))
            if previous is not None:
                raise ValueError(
                    "concurrent retained family animations must target disjoint family leaves; "
                    f"animations {previous} and {animation_index} share one target"
                )
            leaf_owners[id(member)] = animation_index

    # This check runs before lifecycle binding, retained track emission, or canonical
    # family request creation. Same-leaf ownership is therefore rejected transactionally
    # instead of depending on whichever scheduler happens to run first.
    for animation_index, (animation, candidate) in enumerate(
        zip(animations, candidates, strict=True)
    ):
        if candidate is not None:
            continue
        for member in _ordinary_scene_leaves(animation):
            family_owner = leaf_owners.get(id(member))
            if family_owner is not None:
                raise ValueError(
                    "concurrent retained family and ordinary animations must target "
                    "disjoint scene leaves; "
                    f"animations {family_owner} and {animation_index} share one target"
                )

    play_run_time = run_time if run_time is not None else duration
    if play_run_time is not None:
        play_run_time = float(play_run_time)
    play_lag_ratio = None if lag_ratio is None else float(lag_ratio)
    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")

    checkpoint = self._authoring_checkpoint()
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    retained_objects_before = list(getattr(self, "_retained_text_objects", []))
    next_object_id_before = getattr(self, "_retained_next_object_id", None)
    next_order_before = getattr(self, "_retained_next_painter_order", None)
    retained_tracks_before = copy.deepcopy(
        getattr(self, "_retained_animation_tracks", [])
    )
    retained_state_before = copy.deepcopy(
        getattr(self, "_retained_animation_state", {})
    )
    family_requests_before = copy.deepcopy(
        getattr(self, "_retained_family_animations", [])
    )

    geometry_states: dict[int, tuple[_base.Mobject, object, object]] = {}
    retained_states: dict[
        int, tuple[_typst._RetainedTextMobject, object, object, object, object]
    ] = {}
    for animation, candidate, retained_plan in zip(
        animations, candidates, retained_ordinary_plans, strict=True
    ):
        if candidate is None:
            if retained_plan is not None:
                source = _retained._retained_animation_source(animation)
                if source is None:
                    raise RuntimeError("retained animation classification lost its source")
                retained_states[id(source)] = (
                    source,
                    source._scene,
                    source._object,
                    source._retained_object_id,
                    source._retained_order,
                )
            else:
                _animate._record_animation_wrapper_state(animation, geometry_states)
            continue
        _target, _family, leaves, _synthetic = candidate
        for member in leaves:
            if _native_text(member):
                retained_states[id(member)] = (
                    member,
                    member._scene,
                    member._object,
                    member._retained_object_id,
                    member._retained_order,
                )
            else:
                _animate._record_wrapper_state(member, geometry_states)

    try:
        completed: list[tuple[object, object, list[tuple[_base.Mobject, object]], float]] = []
        bound_geometry: list[object] = []
        retained_ordinary_end = base_start

        # Participate in one source-ordered transaction. Family lifecycle/request
        # authoring and direct retained Text property scheduling both own their existing
        # semantics; this layer only coordinates ownership/order and the commit boundary.
        # Geometry semantic-handle commits remain deferred until every sibling succeeds.
        for animation, candidate, retained_plan in zip(
            animations, candidates, retained_ordinary_plans, strict=True
        ):
            if candidate is None:
                if retained_plan is not None:
                    resolved = _options.resolve(
                        builder_args=_options.builder_args(animation),
                        default_lag_ratio=0.0,
                        play_run_time=play_run_time,
                        play_easing=easing,
                        play_rate_func=rate_func,
                        play_lag_ratio=play_lag_ratio,
                    )
                    _retained._schedule_retained_plan(
                        self,
                        animation,
                        retained_plan,
                        start_time=base_start,
                        duration=resolved.run_time,
                        easing=resolved.rate_func,
                    )
                    retained_ordinary_end = max(
                        retained_ordinary_end, base_start + resolved.run_time
                    )
                else:
                    _animate._prepare_aligned_animation_binding(
                        self, animation, start_time=base_start
                    )
                    bound_geometry.append(animation)
                continue

            target, family, leaves, _synthetic = candidate
            lifecycle_plans = _begin_family_lifecycle(
                self,
                animation,
                target,
                leaves,
                start_time=base_start,
            )

            if _is_write_animation(animation):
                assert isinstance(animation, Write)
                (
                    duration_override,
                    lag_override,
                    rate_id,
                    reverse_rate,
                    reverse_members,
                ) = _write_request_inputs(
                    animation,
                    play_run_time=play_run_time,
                    play_easing=easing,
                    play_rate_func=rate_func,
                    play_lag_ratio=play_lag_ratio,
                )
                actual_run_time, _actual_lag = _append_write_request(
                    self,
                    animation,
                    family,
                    leaves,
                    start_time=base_start,
                    duration_override=duration_override,
                    lag_ratio_override=lag_override,
                    rate_function=rate_id,
                    reverse_rate_function=reverse_rate,
                    reverse_member_order=reverse_members,
                )
            else:
                resolved = _options.resolve(
                    builder_args=_options.builder_args(animation),
                    default_lag_ratio=1.0,
                    play_run_time=play_run_time,
                    play_easing=easing,
                    play_rate_func=rate_func,
                    play_lag_ratio=play_lag_ratio,
                )
                if not math.isclose(resolved.path_arc, 0.0, abs_tol=1e-15):
                    raise NotImplementedError(
                        "family Create/Uncreate does not support path_arc"
                    )
                actual_run_time = resolved.run_time
                _append_reveal_request(
                    self,
                    animation,
                    family,
                    leaves,
                    start_time=base_start,
                    duration=resolved.run_time,
                    lag_ratio=resolved.lag_ratio,
                    rate_function=resolved.rate_func,
                )

            completed.append(
                (animation, target, lifecycle_plans, base_start + actual_run_time)
            )

        for animation, target, lifecycle_plans, end_time in completed:
            _finish_family_lifecycle(
                self,
                animation,
                target,
                lifecycle_plans,
                end_time=end_time,
            )

        geometry_end, semantic_targets = _animate._schedule_aligned_bound_animations(
            self,
            bound_geometry,
            base_start=base_start,
            play_run_time=play_run_time,
            play_easing=easing,
            play_rate_func=rate_func,
            play_lag_ratio=play_lag_ratio,
        )
        play_end = max(
            [geometry_end, retained_ordinary_end]
            + [end_time for _animation, _target, _plans, end_time in completed]
        )
        self._cursor = max(cursor_before, play_end)

        # Deliberately last: neither family requests nor retained Text tracks can fail
        # after ordinary semantic handles are committed. A source edit/rerun therefore
        # starts from its fresh Scene/context rather than leaked target state.
        _animate._commit_semantic_targets(semantic_targets)
        return self
    except Exception:
        # #882's Scene checkpoint restores the Rust-owned canonical authoring context;
        # the sidecar/wrapper restoration below covers the retained compatibility views.
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
        for (
            member,
            old_scene,
            old_object,
            old_object_id,
            old_order,
        ) in retained_states.values():
            member._scene = old_scene
            member._object = old_object
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
    """Install after ordinary retained and DrawBorderThenFill Scene.play wrappers."""

    global _INSTALLED, _ORIGINAL_SCENE_PLAY, _ORIGINAL_RETAINED_DOCUMENT
    if _INSTALLED:
        return
    _INSTALLED = True

    for name, value in {"Write": Write, "Unwrite": Unwrite}.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)

    _ORIGINAL_SCENE_PLAY = _compat.Scene.play
    _ORIGINAL_RETAINED_DOCUMENT = _compat.Scene.retained_document
    _compat.Scene.play = _family_scene_play
    _compat.Scene.retained_document = _retained_document
