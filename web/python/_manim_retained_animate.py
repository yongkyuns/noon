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
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
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

    raise NotImplementedError(
        f"retained Text .animate.{name} is not supported yet; "
        "position, rotation, and uniform scale animations are supported"
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


def _retained_animation_plan(animation: object) -> list[dict[str, Any]] | None:
    """Return replayable retained animation intent, or ``None`` for legacy animation."""

    if not isinstance(animation, _animate._AlignedAnimationBuilder):
        return None
    source = getattr(animation, "source", None)
    if not isinstance(source, _typst._RetainedTextMobject):
        return None

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


def _bind_retained(scene: _compat.Scene, source: _typst._RetainedTextMobject) -> None:
    if source._scene is None:
        _typst._add_retained(scene, source, None)
    elif source._scene is not scene:
        raise ValueError("retained text Mobject already belongs to another Scene")
    else:
        scene._register_top_level(source)


def _initial_animation_state(source: _typst._RetainedTextMobject) -> dict[str, Any]:
    transform = _transform(source._spec())
    return {
        "position": _vec2(transform.get("translation"), "translation"),
        "rotation": _finite_scalar(transform.get("rotation"), "rotation"),
        "scale": _vec2(transform.get("scale"), "scale"),
    }


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


def _schedule_retained_plan(
    scene: _compat.Scene,
    animation: object,
    operations: list[dict[str, Any]],
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    source = animation.source
    _bind_retained(scene, source)
    _ensure_animation_state(scene)

    object_id = int(source.id)
    state = scene._retained_animation_state.setdefault(
        object_id, _initial_animation_state(source)
    )
    current_position = copy.deepcopy(state["position"])
    current_rotation = float(state["rotation"])
    current_scale = copy.deepcopy(state["scale"])
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
            target_scale = {
                "x": float(target_scale["x"]) * factor,
                "y": float(target_scale["y"]) * factor,
            }
            continue
        if kind == "shift_relative":
            if "position" not in touched:
                touched.append("position")
            delta = operation["delta"]
            target_position = {
                "x": float(target_position["x"]) + float(delta["x"]),
                "y": float(target_position["y"]) + float(delta["y"]),
            }
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
        raise ValueError(f"unknown retained animation operation {kind!r}")

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
    wrapper_states = {
        id(animation.source): (
            animation.source,
            animation.source._scene,
            animation.source._retained_object_id,
            animation.source._retained_order,
        )
        for animation in animations
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
    _compat.Scene.play = _retained_scene_play
    _compat.Scene.retained_document = _retained_document
