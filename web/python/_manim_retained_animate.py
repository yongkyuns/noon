"""Retained source-level animation scheduling for Text/Typst objects.

This module extends the ordinary Manim ``Scene.play`` scheduler only for retained
resource-backed objects. It emits backend-neutral semantic tracks into the retained
authoring sidecar; it never synthesizes legacy geometry or frontend-owned track IDs.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import _manim_animate as _animate
import _manim_animation_options as _options
import _manim_compat as _compat
import _manim_lifecycle as _lifecycle
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_ADD = _compat.Scene.add
_ORIGINAL_SCENE_REMOVE = _compat.Scene.remove
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_SCENE_IS_PRESENT = _compat.Scene._is_present
_ORIGINAL_RETAINED_DOCUMENT = _compat.Scene.retained_document
_ORIGINAL_BUILDER_GETATTR = _animate._AlignedAnimationBuilder.__getattr__


def _retained_copy_for_animate_target(
    self: _typst._RetainedTextMobject,
) -> _typst._RetainedTextMobject:
    """Clone a retained animation target without entering geometry-only cloning."""

    return self.copy()


def _ensure_animation_state(scene: _compat.Scene) -> None:
    _typst._ensure_scene_state(scene)
    if not hasattr(scene, "_retained_animation_tracks"):
        scene._retained_animation_tracks = []
        scene._retained_animation_state = {}


def _vec2(value: object, label: str) -> dict[str, float]:
    if not isinstance(value, dict):
        raise ValueError(f"retained text authoring spec is missing {label} state")
    x = float(value["x"])
    y = float(value["y"])
    if not math.isfinite(x) or not math.isfinite(y):
        raise ValueError(f"retained text {label} state must be finite")
    return {"x": x, "y": y}


def _finite_scalar(value: object, label: str) -> float:
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"retained text {label} state must be finite")
    return result


def _unit_scalar(value: object, label: str) -> float:
    result = _finite_scalar(value, label)
    if result < 0.0 or result > 1.0:
        raise ValueError(f"retained text {label} state must be between zero and one")
    return result


def _transform(spec: dict[str, Any]) -> dict[str, Any]:
    transform = spec.get("transform")
    if not isinstance(transform, dict):
        raise ValueError("retained text authoring spec is missing transform state")
    return transform


def _assert_only_transform_field_changed(
    before: dict[str, Any],
    after: dict[str, Any],
    field: str,
) -> None:
    before_normalized = copy.deepcopy(before)
    after_normalized = copy.deepcopy(after)
    before_normalized["transform"][field] = None
    after_normalized["transform"][field] = None
    if before_normalized != after_normalized:
        raise NotImplementedError(
            f"retained Text .animate.{field} changed unsupported retained state"
        )


def _assert_only_spec_field_changed(
    before: dict[str, Any],
    after: dict[str, Any],
    field: str,
    method: str,
) -> None:
    before_normalized = copy.deepcopy(before)
    after_normalized = copy.deepcopy(after)
    before_normalized[field] = None
    after_normalized[field] = None
    if before_normalized != after_normalized:
        raise NotImplementedError(
            f"retained Text .animate.{method} changed unsupported retained state"
        )


def _scale_factor_between(before: dict[str, Any], after: dict[str, Any]) -> float:
    before_scale = _vec2(_transform(before).get("scale"), "scale")
    after_scale = _vec2(_transform(after).get("scale"), "scale")
    if math.isclose(before_scale["x"], 0.0, abs_tol=1e-15) or math.isclose(
        before_scale["y"], 0.0, abs_tol=1e-15
    ):
        raise ValueError("retained Text .animate cannot derive scale from a zero-scale source")

    factor_x = after_scale["x"] / before_scale["x"]
    factor_y = after_scale["y"] / before_scale["y"]
    if not math.isclose(factor_x, factor_y, rel_tol=1e-12, abs_tol=1e-12):
        raise NotImplementedError("retained Text .animate supports uniform scale only")
    factor = (factor_x + factor_y) / 2.0
    if not math.isfinite(factor) or factor <= 0.0:
        raise ValueError("retained Text .animate scale factor must be finite and positive")
    return factor


def _semantic_operation(
    name: str,
    before: dict[str, Any],
    after: dict[str, Any],
) -> dict[str, Any]:
    """Normalize one retained builder method into replayable semantic intent."""

    if name == "scale":
        _assert_only_transform_field_changed(before, after, "scale")
        return {"kind": "scale_relative", "factor": _scale_factor_between(before, after)}

    if name == "shift":
        _assert_only_transform_field_changed(before, after, "translation")
        before_position = _vec2(_transform(before).get("translation"), "translation")
        after_position = _vec2(_transform(after).get("translation"), "translation")
        return {
            "kind": "shift_relative",
            "delta": {
                "x": after_position["x"] - before_position["x"],
                "y": after_position["y"] - before_position["y"],
            },
        }

    if name in {"move_to", "center"}:
        _assert_only_transform_field_changed(before, after, "translation")
        return {
            "kind": "move_to",
            "position": _vec2(_transform(after).get("translation"), "translation"),
        }

    if name == "set_x":
        _assert_only_transform_field_changed(before, after, "translation")
        position = _vec2(_transform(after).get("translation"), "translation")
        return {"kind": "set_x", "x": position["x"]}

    if name == "set_y":
        _assert_only_transform_field_changed(before, after, "translation")
        position = _vec2(_transform(after).get("translation"), "translation")
        return {"kind": "set_y", "y": position["y"]}

    if name == "rotate":
        _assert_only_transform_field_changed(before, after, "rotation")
        before_rotation = _finite_scalar(_transform(before).get("rotation"), "rotation")
        after_rotation = _finite_scalar(_transform(after).get("rotation"), "rotation")
        return {"kind": "rotate_relative", "angle": after_rotation - before_rotation}

    if name == "set_opacity":
        _assert_only_spec_field_changed(before, after, "opacity", name)
        return {"kind": "opacity_to", "opacity": _unit_scalar(after.get("opacity"), "opacity")}

    raise NotImplementedError(
        f"retained Text .animate.{name} is not supported yet; "
        "position, rotation, opacity, and uniform scale animations are supported"
    )


def _retained_builder_getattr(
    self: _animate._AlignedAnimationBuilder,
    name: str,
):
    """Record retained method intent while preserving the generic Manim builder."""

    invoke = _ORIGINAL_BUILDER_GETATTR(self, name)
    source = getattr(self, "source", None)
    if not isinstance(source, _typst._RetainedTextMobject):
        return invoke

    def retained_invoke(*args: Any, **kwargs: Any):
        before = self.target._spec()
        result = invoke(*args, **kwargs)
        after = self.target._spec()
        operations = vars(self).setdefault("_retained_method_operations", [])
        operations.append(_semantic_operation(name, before, after))
        return result

    return retained_invoke


def _retained_animation_source(
    animation: object,
) -> _typst._RetainedTextMobject | None:
    if isinstance(animation, _animate._AlignedAnimationBuilder):
        source = getattr(animation, "source", None)
    elif isinstance(animation, (_animate.FadeIn, _animate.FadeOut)):
        source = getattr(animation, "target", None)
    else:
        return None
    return source if isinstance(source, _typst._RetainedTextMobject) else None


def _retained_animation_plan(animation: object) -> list[dict[str, Any]] | None:
    """Return replayable retained animation intent, or ``None`` for legacy animation."""

    source = _retained_animation_source(animation)
    if source is None:
        return None

    if isinstance(animation, (_animate.FadeIn, _animate.FadeOut)):
        shift = _compat._as_vec2(animation._fade_shift_vector)
        scale_factor = float(animation._fade_scale_factor)
        if not math.isfinite(scale_factor):
            raise ValueError("retained text fade scale must be finite")
        return [
            {
                "kind": "fade_in" if isinstance(animation, _animate.FadeIn) else "fade_out",
                "shift": {"x": float(shift.x), "y": float(shift.y)},
                "scale_factor": scale_factor,
                "point_target": bool(animation._fade_point_target),
            }
        ]

    deferred_factor = vars(animation).get("scale_factor")
    if deferred_factor is not None:
        factor = float(deferred_factor)
        if not math.isfinite(factor):
            raise ValueError("retained text scale factor must be finite")
        return [{"kind": "scale_relative", "factor": factor}]

    target = animation.target
    if type(target) is not type(source):
        raise NotImplementedError(
            "retained Text .animate currently requires a target of the same retained text type"
        )

    operations = vars(animation).get("_retained_method_operations", [])
    if not isinstance(operations, list):
        raise TypeError("retained animation method log must be a list")
    return copy.deepcopy(operations)


def _bind_retained(
    scene: _compat.Scene,
    source: _typst._RetainedTextMobject,
) -> None:
    if source._scene is None:
        _typst._add_retained(scene, source, None)
    elif source._scene is not scene:
        raise ValueError("retained text Mobject already belongs to another Scene")
    else:
        scene._register_top_level(source)


def _initial_animation_state(source: _typst._RetainedTextMobject) -> dict[str, Any]:
    spec = source._spec()
    transform = _transform(spec)
    return {
        "appearance": 1.0,
        "opacity": _unit_scalar(spec.get("opacity"), "opacity"),
        "position": _vec2(transform.get("translation"), "translation"),
        "presence": True,
        "rotation": _finite_scalar(transform.get("rotation"), "rotation"),
        "scale": _vec2(transform.get("scale"), "scale"),
        "runtime_position": _vec2(transform.get("translation"), "translation"),
        "runtime_scale": _vec2(transform.get("scale"), "scale"),
    }


def _retained_presence_tracks(
    scene: _compat.Scene,
    object_id: int,
) -> list[dict[str, Any]]:
    return [
        track
        for track in getattr(scene, "_retained_animation_tracks", [])
        if int(track["object"]) == object_id and track["property"] == "presence"
    ]


def _resolve_retained_lifecycle(
    scene: _compat.Scene,
    source: _typst._RetainedTextMobject,
    intent: str,
    time: float,
    label: str,
) -> _lifecycle.LifecyclePlan:
    _ensure_animation_state(scene)
    if source._scene is None:
        return _lifecycle._resolve(
            intent,
            binding="detached",
            has_presence_timeline=False,
            present=True,
            has_future_event=False,
            at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
            label=label,
        )
    if source._scene is not scene:
        return _lifecycle._resolve(
            intent,
            binding="other_scene",
            has_presence_timeline=False,
            present=True,
            has_future_event=False,
            at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
            label=label,
        )

    object_id = int(source.id)
    tracks = _retained_presence_tracks(scene, object_id)
    state = scene._retained_animation_state.get(object_id)
    present = True if state is None else bool(state["presence"])
    has_future = any(float(track["timing"]["start_time"]) > time for track in tracks)
    return _lifecycle._resolve(
        intent,
        binding="this_scene",
        has_presence_timeline=bool(tracks),
        present=present,
        has_future_event=has_future,
        at_time_zero=math.isclose(time, 0.0, abs_tol=1e-12),
        label=label,
    )


def _append_vec2_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    property_name: str,
    current: dict[str, float],
    target: dict[str, float],
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    scene._retained_animation_tracks.append(
        {
            "object": object_id,
            "property": property_name,
            "values": {
                "vec2": {
                    "from": {"x": float(current["x"]), "y": float(current["y"])},
                    "to": {"x": float(target["x"]), "y": float(target["y"])},
                }
            },
            "timing": {
                "start_time": float(start_time),
                "duration": float(duration),
                "easing": str(easing),
            },
        }
    )


def _append_scalar_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    property_name: str,
    current: float,
    target: float,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    scene._retained_animation_tracks.append(
        {
            "object": object_id,
            "property": property_name,
            "values": {"scalar": {"from": float(current), "to": float(target)}},
            "timing": {
                "start_time": float(start_time),
                "duration": float(duration),
                "easing": str(easing),
            },
        }
    )


def _append_presence_track(
    scene: _compat.Scene,
    *,
    object_id: int,
    current: bool,
    target: bool,
    start_time: float,
) -> None:
    existing = _retained_presence_tracks(scene, object_id)
    previous = existing[-1] if existing else None
    result = _lifecycle._validate_shared_presence_transition(
        previous is not None,
        0.0 if previous is None else float(previous["timing"]["start_time"]),
        False if previous is None else bool(previous["values"]["bool"]["to"]),
        float(start_time),
        bool(current),
    )
    if not bool(result.ok):
        raise ValueError(str(result.message))

    scene._retained_animation_tracks.append(
        {
            "object": object_id,
            "property": "presence",
            "values": {"bool": {"from": bool(current), "to": bool(target)}},
            "timing": {
                "start_time": float(start_time),
                "duration": 0.0,
                "easing": "linear",
            },
        }
    )


def _restore_reintroduced_runtime_state(
    scene: _compat.Scene,
    *,
    object_id: int,
    state: dict[str, Any],
    start_time: float,
    duration: float,
    touched: tuple[str, ...] | list[str] = (),
) -> None:
    """Restore transient fade endpoints while a reintroduction animation runs."""

    current_position = copy.deepcopy(state["position"])
    current_scale = copy.deepcopy(state["scale"])
    if not math.isclose(float(state["appearance"]), 1.0, abs_tol=1e-15):
        _append_scalar_track(
            scene,
            object_id=object_id,
            property_name="appearance",
            current=1.0,
            target=1.0,
            start_time=start_time,
            duration=duration,
            easing="linear",
        )
    if state["runtime_position"] != current_position and "position" not in touched:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="position",
            current=current_position,
            target=current_position,
            start_time=start_time,
            duration=duration,
            easing="linear",
        )
    if state["runtime_scale"] != current_scale and "scale" not in touched:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="scale",
            current=current_scale,
            target=current_scale,
            start_time=start_time,
            duration=duration,
            easing="linear",
        )
    state["appearance"] = 1.0
    state["runtime_position"] = copy.deepcopy(current_position)
    state["runtime_scale"] = copy.deepcopy(current_scale)


def _unique_retained_mobjects(
    mobjects: tuple[object, ...],
) -> list[_typst._RetainedTextMobject]:
    retained: list[_typst._RetainedTextMobject] = []
    identities: set[int] = set()
    for value in mobjects:
        if not isinstance(value, _typst._RetainedTextMobject) or id(value) in identities:
            continue
        identities.add(id(value))
        retained.append(value)
    return retained


def _retained_scene_add(
    self: _compat.Scene,
    *mobjects: object,
    key: str | None = None,
):
    retained = _unique_retained_mobjects(mobjects)
    if not retained:
        return _ORIGINAL_SCENE_ADD(self, *mobjects, key=key)

    plans = [
        (
            value,
            _resolve_retained_lifecycle(
                self,
                value,
                "add",
                self._cursor,
                "Scene.add target",
            ),
        )
        for value in retained
    ]
    result = _ORIGINAL_SCENE_ADD(self, *mobjects, key=key)
    _ensure_animation_state(self)
    for value, plan in plans:
        object_id = int(value.id)
        state = self._retained_animation_state.setdefault(
            object_id, _initial_animation_state(value)
        )
        if plan.show_now:
            state["presence"] = False
            _append_presence_track(
                self,
                object_id=object_id,
                current=False,
                target=True,
                start_time=self._cursor,
            )
            state["presence"] = True
        else:
            state["presence"] = True
    return result


def _retained_scene_remove(self: _compat.Scene, *mobjects: object) -> _compat.Scene:
    retained = _unique_retained_mobjects(mobjects)
    if not retained:
        return _ORIGINAL_SCENE_REMOVE(self, *mobjects)

    plans = [
        (
            value,
            _resolve_retained_lifecycle(
                self,
                value,
                "remove",
                self._cursor,
                "Scene.remove target",
            ),
        )
        for value in retained
    ]
    _ensure_animation_state(self)
    for value, plan in plans:
        if value._scene is not self or value._retained_object_id is None:
            continue
        object_id = int(value.id)
        state = self._retained_animation_state.setdefault(
            object_id, _initial_animation_state(value)
        )
        if plan.hide_now:
            _append_presence_track(
                self,
                object_id=object_id,
                current=True,
                target=False,
                start_time=self._cursor,
            )
            state["presence"] = False

    legacy = tuple(
        value for value in mobjects if not isinstance(value, _typst._RetainedTextMobject)
    )
    if legacy:
        _ORIGINAL_SCENE_REMOVE(self, *legacy)
    retained_identities = {id(value) for value in retained}
    self._compat_top_level = [
        value for value in self._compat_top_level if id(value) not in retained_identities
    ]
    return self


def _offset_vec2(
    value: dict[str, float],
    delta: dict[str, float],
    factor: float = 1.0,
) -> dict[str, float]:
    return {
        "x": float(value["x"]) + float(delta["x"]) * factor,
        "y": float(value["y"]) + float(delta["y"]) * factor,
    }


def _scale_vec2(value: dict[str, float], factor: float) -> dict[str, float]:
    return {
        "x": float(value["x"]) * factor,
        "y": float(value["y"]) * factor,
    }


def _schedule_retained_fade(
    scene: _compat.Scene,
    *,
    object_id: int,
    state: dict[str, Any],
    operation: dict[str, Any],
    lifecycle: _lifecycle.LifecyclePlan,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    """Lower one fade without changing the retained object's canonical presentation."""

    fade_in = operation["kind"] == "fade_in"
    shift = _vec2(operation["shift"], "fade shift")
    scale_factor = _finite_scalar(operation["scale_factor"], "fade scale")
    point_target = bool(operation["point_target"])
    canonical_position = copy.deepcopy(state["position"])
    canonical_scale = copy.deepcopy(state["scale"])

    if lifecycle.show_at_start:
        state["presence"] = False
        _append_presence_track(
            scene,
            object_id=object_id,
            current=False,
            target=True,
            start_time=start_time,
        )
        state["presence"] = True

    shift_is_zero = math.isclose(shift["x"], 0.0, abs_tol=1e-15) and math.isclose(
        shift["y"], 0.0, abs_tol=1e-15
    )
    scale_is_identity = math.isclose(scale_factor, 1.0, rel_tol=0.0, abs_tol=1e-15)

    if fade_in:
        # Manim's faded starting copy moves opposite ``shift`` unless
        # target_position supplied the point explicitly.
        direction = 1.0 if point_target else -1.0
        faded_position = (
            canonical_position
            if shift_is_zero
            else _offset_vec2(canonical_position, shift, direction)
        )
        faded_scale = (
            canonical_scale
            if scale_is_identity
            else _scale_vec2(canonical_scale, scale_factor)
        )

        if (
            faded_position != canonical_position
            or state["runtime_position"] != canonical_position
        ):
            _append_vec2_track(
                scene,
                object_id=object_id,
                property_name="position",
                current=faded_position,
                target=canonical_position,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
        if faded_scale != canonical_scale or state["runtime_scale"] != canonical_scale:
            _append_vec2_track(
                scene,
                object_id=object_id,
                property_name="scale",
                current=faded_scale,
                target=canonical_scale,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
        _append_scalar_track(
            scene,
            object_id=object_id,
            property_name="appearance",
            current=0.0,
            target=1.0,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
        state["appearance"] = 1.0
        state["presence"] = True
        state["runtime_position"] = copy.deepcopy(canonical_position)
        state["runtime_scale"] = copy.deepcopy(canonical_scale)
        return

    faded_position = (
        canonical_position
        if shift_is_zero
        else _offset_vec2(canonical_position, shift)
    )
    faded_scale = (
        canonical_scale
        if scale_is_identity
        else _scale_vec2(canonical_scale, scale_factor)
    )
    if (
        faded_position != canonical_position
        or state["runtime_position"] != canonical_position
    ):
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="position",
            current=canonical_position,
            target=faded_position,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
    if faded_scale != canonical_scale or state["runtime_scale"] != canonical_scale:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="scale",
            current=canonical_scale,
            target=faded_scale,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
    _append_scalar_track(
        scene,
        object_id=object_id,
        property_name="appearance",
        current=float(state["appearance"]),
        target=0.0,
        start_time=start_time,
        duration=duration,
        easing=easing,
    )
    end_time = start_time + duration
    if lifecycle.hide_at_end:
        _append_presence_track(
            scene,
            object_id=object_id,
            current=True,
            target=False,
            start_time=end_time,
        )
        state["presence"] = False

    # Manim FadeOut cleanup removes the object, then restores interpolation alpha 0.
    # Keep that restoration in the ordinary property channels so direct seek and
    # later Scene.add observe canonical state without Python-only repair state.
    _append_scalar_track(
        scene,
        object_id=object_id,
        property_name="appearance",
        current=0.0,
        target=1.0,
        start_time=end_time,
        duration=0.0,
        easing="linear",
    )
    if faded_position != canonical_position:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="position",
            current=faded_position,
            target=canonical_position,
            start_time=end_time,
            duration=0.0,
            easing="linear",
        )
    if faded_scale != canonical_scale:
        _append_vec2_track(
            scene,
            object_id=object_id,
            property_name="scale",
            current=faded_scale,
            target=canonical_scale,
            start_time=end_time,
            duration=0.0,
            easing="linear",
        )
    state["appearance"] = 1.0
    state["runtime_position"] = copy.deepcopy(canonical_position)
    state["runtime_scale"] = copy.deepcopy(canonical_scale)


def _schedule_retained_plan(
    scene: _compat.Scene,
    animation: object,
    operations: list[dict[str, Any]],
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    source = _retained_animation_source(animation)
    if source is None:
        raise TypeError("retained animation lost its retained Text source")

    fade_kind = (
        operations[0]["kind"]
        if len(operations) == 1 and operations[0]["kind"] in {"fade_in", "fade_out"}
        else None
    )

    if fade_kind == "fade_in":
        lifecycle = _resolve_retained_lifecycle(
            scene, source, "introduce", start_time, "fade target"
        )
        _bind_retained(scene, source)
    elif fade_kind == "fade_out":
        add_plan = _resolve_retained_lifecycle(
            scene, source, "add", start_time, "animated fade target"
        )
        _bind_retained(scene, source)
    else:
        add_plan = _resolve_retained_lifecycle(
            scene, source, "add", start_time, "animated Mobject"
        )
        _bind_retained(scene, source)

    _ensure_animation_state(scene)
    object_id = int(source.id)
    state = scene._retained_animation_state.setdefault(
        object_id, _initial_animation_state(source)
    )

    if fade_kind == "fade_out":
        if add_plan.show_now:
            state["presence"] = False
            _append_presence_track(
                scene,
                object_id=object_id,
                current=False,
                target=True,
                start_time=start_time,
            )
            state["presence"] = True
            # FadeOut follows Manim's normal Scene.play implicit-add behavior:
            # an absent object is restored to its canonical visible state before
            # the transient faded endpoint is evaluated.
            state["appearance"] = 1.0
        lifecycle = _resolve_retained_lifecycle(
            scene, source, "remove_after_animation", start_time, "fade target"
        )
        _schedule_retained_fade(
            scene,
            object_id=object_id,
            state=state,
            operation=operations[0],
            lifecycle=lifecycle,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
        return

    if fade_kind == "fade_in":
        _schedule_retained_fade(
            scene,
            object_id=object_id,
            state=state,
            operation=operations[0],
            lifecycle=lifecycle,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
        return

    reintroducing = bool(add_plan.show_now)
    if reintroducing:
        state["presence"] = False
        _append_presence_track(
            scene,
            object_id=object_id,
            current=False,
            target=True,
            start_time=start_time,
        )
        state["presence"] = True

    current_opacity = float(state["opacity"])
    current_position = copy.deepcopy(state["position"])
    current_rotation = float(state["rotation"])
    current_scale = copy.deepcopy(state["scale"])
    target_opacity = current_opacity
    target_position = copy.deepcopy(current_position)
    target_rotation = current_rotation
    target_scale = copy.deepcopy(current_scale)
    touched: list[str] = []

    for operation in operations:
        kind = operation["kind"]
        if kind == "scale_relative":
            if "scale" not in touched:
                touched.append("scale")
            factor = float(operation["factor"])
            target_scale = _scale_vec2(target_scale, factor)
            continue
        if kind == "shift_relative":
            if "position" not in touched:
                touched.append("position")
            target_position = _offset_vec2(target_position, operation["delta"])
            continue
        if kind == "move_to":
            if "position" not in touched:
                touched.append("position")
            target_position = copy.deepcopy(operation["position"])
            continue
        if kind == "set_x":
            if "position" not in touched:
                touched.append("position")
            target_position["x"] = float(operation["x"])
            continue
        if kind == "set_y":
            if "position" not in touched:
                touched.append("position")
            target_position["y"] = float(operation["y"])
            continue
        if kind == "rotate_relative":
            if "rotation" not in touched:
                touched.append("rotation")
            target_rotation += float(operation["angle"])
            continue
        if kind == "opacity_to":
            if "opacity" not in touched:
                touched.append("opacity")
            target_opacity = _unit_scalar(operation["opacity"], "opacity")
            continue
        raise ValueError(f"unknown retained animation operation {kind!r}")

    if reintroducing:
        _restore_reintroduced_runtime_state(
            scene,
            object_id=object_id,
            state=state,
            start_time=start_time,
            duration=duration,
            touched=touched,
        )

    for property_name in touched:
        if property_name == "scale":
            _append_vec2_track(
                scene,
                object_id=object_id,
                property_name="scale",
                current=current_scale,
                target=target_scale,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
            state["scale"] = copy.deepcopy(target_scale)
            state["runtime_scale"] = copy.deepcopy(target_scale)
        elif property_name == "position":
            _append_vec2_track(
                scene,
                object_id=object_id,
                property_name="position",
                current=current_position,
                target=target_position,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
            state["position"] = copy.deepcopy(target_position)
            state["runtime_position"] = copy.deepcopy(target_position)
        elif property_name == "rotation":
            _append_scalar_track(
                scene,
                object_id=object_id,
                property_name="rotation",
                current=current_rotation,
                target=target_rotation,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
            state["rotation"] = target_rotation
        elif property_name == "opacity":
            _append_scalar_track(
                scene,
                object_id=object_id,
                property_name="opacity",
                current=current_opacity,
                target=target_opacity,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
            state["opacity"] = target_opacity


def _retained_scene_is_present(self: _compat.Scene, value: object) -> bool:
    if not isinstance(value, _typst._RetainedTextMobject):
        return _ORIGINAL_SCENE_IS_PRESENT(self, value)
    if value._scene is not self or value._retained_object_id is None:
        return False
    state = getattr(self, "_retained_animation_state", {}).get(int(value._retained_object_id))
    return True if state is None else bool(state["presence"])


def _retained_document(self: _compat.Scene) -> dict[str, Any]:
    document = _ORIGINAL_RETAINED_DOCUMENT(self)
    tracks = getattr(self, "_retained_animation_tracks", None)
    if tracks:
        document["tracks"] = copy.deepcopy(tracks)
    return document


def _retained_scene_play(
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
    retained_plans = [_retained_animation_plan(animation) for animation in animations]
    if not any(plan is not None for plan in retained_plans):
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
    if any(plan is None for plan in retained_plans):
        raise NotImplementedError(
            "mixing retained Text animations with legacy animations in one Scene.play "
            "is not supported yet"
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

    _ensure_animation_state(self)
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    retained_objects_before = list(self._retained_text_objects)
    next_object_id_before = self._retained_next_object_id
    next_order_before = self._retained_next_painter_order
    tracks_before = copy.deepcopy(self._retained_animation_tracks)
    state_before = copy.deepcopy(self._retained_animation_state)
    retained_sources = [
        source
        for animation in animations
        if (source := _retained_animation_source(animation)) is not None
    ]
    wrapper_states = {
        id(source): (
            source,
            source._scene,
            source._retained_object_id,
            source._retained_order,
        )
        for source in retained_sources
    }

    max_end = base_start
    try:
        for animation, plan in zip(animations, retained_plans):
            assert plan is not None
            resolved = _options.resolve(
                builder_args=_options.builder_args(animation),
                default_lag_ratio=0.0,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule_retained_plan(
                self,
                animation,
                plan,
                start_time=base_start,
                duration=resolved.run_time,
                easing=resolved.rate_func,
            )
            max_end = max(max_end, base_start + resolved.run_time)

        self._cursor = max(cursor_before, max_end)
        return self
    except Exception:
        self._cursor = cursor_before
        self._compat_top_level = top_level_before
        self._retained_text_objects = retained_objects_before
        self._retained_next_object_id = next_object_id_before
        self._retained_next_painter_order = next_order_before
        self._retained_animation_tracks = tracks_before
        self._retained_animation_state = state_before
        for source, old_scene, old_object_id, old_order in wrapper_states.values():
            source._scene = old_scene
            source._retained_object_id = old_object_id
            source._retained_order = old_order
        raise


def install() -> None:
    """Install retained animation scheduling after retained Text Scene hooks."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True
    _typst._RetainedTextMobject._copy_for_animate_target = _retained_copy_for_animate_target
    _animate._AlignedAnimationBuilder.__getattr__ = _retained_builder_getattr
    _compat.Scene.add = _retained_scene_add
    _compat.Scene.remove = _retained_scene_remove
    _compat.Scene._is_present = _retained_scene_is_present
    _compat.Scene.play = _retained_scene_play
    _compat.Scene.retained_document = _retained_document
