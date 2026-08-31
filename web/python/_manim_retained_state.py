"""Keep retained Text authoring state canonical across direct edits and animations.

This is a migration seam for the retained sidecar. The Rust/WASM retained handle owns
current authoring state; the retained timeline keeps the immutable/base state plus
tracks. Direct edits after time has advanced are therefore lowered to exact instant
property assignments instead of retroactively rewriting the object's time-zero spec.

The module deliberately does not own layout, animation endpoints, lifecycle, or
runtime evaluation. Those remain in the shared Rust handles and retained scheduler.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import _manim_compat as _compat
import _manim_retained_animate as _retained
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_ADD = _compat.Scene.add
_ORIGINAL_SCENE_REMOVE = _compat.Scene.remove
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_SCENE_WAIT = _compat.Scene.wait
_ORIGINAL_RETAINED_DOCUMENT = _compat.Scene.retained_document


def _bound_sources(scene: _compat.Scene) -> list[_typst._RetainedTextMobject]:
    return [
        source
        for source in getattr(scene, "_retained_text_objects", [])
        if isinstance(source, _typst._RetainedTextMobject)
        and source._scene is scene
        and source._retained_object_id is not None
    ]


def _freeze_base(source: _typst._RetainedTextMobject) -> dict[str, Any]:
    base = getattr(source, "_retained_timeline_base_spec", None)
    if base is None:
        base = copy.deepcopy(source._spec())
        source._retained_timeline_base_spec = base
    return base


def _observe_source(source: _typst._RetainedTextMobject) -> dict[str, Any]:
    observed = copy.deepcopy(source._spec())
    source._retained_observed_spec = observed
    return observed


def _observed_source(source: _typst._RetainedTextMobject) -> dict[str, Any]:
    observed = getattr(source, "_retained_observed_spec", None)
    if observed is None:
        observed = copy.deepcopy(_freeze_base(source))
        source._retained_observed_spec = observed
    return observed


def _freeze_bound_sources(scene: _compat.Scene) -> None:
    for source in _bound_sources(scene):
        _freeze_base(source)
        if not hasattr(source, "_retained_observed_spec"):
            _observe_source(source)


def _color(spec: dict[str, Any]) -> dict[str, float]:
    value = spec.get("color")
    if not isinstance(value, dict):
        raise ValueError("retained text authoring spec is missing color state")
    result = {
        "red": float(value["red"]),
        "green": float(value["green"]),
        "blue": float(value["blue"]),
        "alpha": float(value["alpha"]),
    }
    if any(not math.isfinite(component) for component in result.values()):
        raise ValueError("retained text color state must be finite")
    return result


def _state_from_spec(spec: dict[str, Any]) -> dict[str, Any]:
    transform = _retained._transform(spec)
    return {
        "appearance": 1.0,
        "color": _color(spec),
        "opacity": _retained._unit_scalar(spec.get("opacity"), "opacity"),
        "position": _retained._vec2(transform.get("translation"), "translation"),
        "presence": True,
        "rotation": _retained._finite_scalar(transform.get("rotation"), "rotation"),
        "scale": _retained._vec2(transform.get("scale"), "scale"),
        "runtime_position": _retained._vec2(transform.get("translation"), "translation"),
        "runtime_scale": _retained._vec2(transform.get("scale"), "scale"),
    }


def _state_for(
    scene: _compat.Scene,
    source: _typst._RetainedTextMobject,
) -> dict[str, Any]:
    _retained._ensure_animation_state(scene)
    object_id = int(source.id)
    state = scene._retained_animation_state.get(object_id)
    if state is None:
        state = _state_from_spec(_freeze_base(source))
        scene._retained_animation_state[object_id] = state
    else:
        state.setdefault("color", _color(_freeze_base(source)))
    return state


def _same_scalar(left: float, right: float) -> bool:
    return math.isclose(float(left), float(right), rel_tol=1e-6, abs_tol=1e-6)


def _same_vec2(left: dict[str, float], right: dict[str, float]) -> bool:
    return _same_scalar(left["x"], right["x"]) and _same_scalar(left["y"], right["y"])


def _same_color(left: dict[str, float], right: dict[str, float]) -> bool:
    return all(_same_scalar(left[channel], right[channel]) for channel in left)


def _object_has_tracks(scene: _compat.Scene, object_id: int) -> bool:
    return any(
        int(track["object"]) == object_id
        for track in getattr(scene, "_retained_animation_tracks", [])
    )


def _update_state_from_spec(state: dict[str, Any], spec: dict[str, Any]) -> None:
    transform = _retained._transform(spec)
    position = _retained._vec2(transform.get("translation"), "translation")
    scale = _retained._vec2(transform.get("scale"), "scale")
    state["color"] = _color(spec)
    state["opacity"] = _retained._unit_scalar(spec.get("opacity"), "opacity")
    state["position"] = copy.deepcopy(position)
    state["rotation"] = _retained._finite_scalar(transform.get("rotation"), "rotation")
    state["scale"] = copy.deepcopy(scale)
    state["runtime_position"] = copy.deepcopy(position)
    state["runtime_scale"] = copy.deepcopy(scale)


def _validate_static_source_contract(
    base: dict[str, Any],
    current: dict[str, Any],
) -> None:
    for field in ("source", "backend", "font_size"):
        if current.get(field) != base.get(field):
            raise NotImplementedError(
                f"retained text direct {field} mutation requires shared localized resource mutation"
            )


def _sync_source(
    scene: _compat.Scene,
    source: _typst._RetainedTextMobject,
    at_time: float,
) -> None:
    if source._scene is not scene or source._retained_object_id is None:
        return

    object_id = int(source.id)
    base = _freeze_base(source)
    previous = copy.deepcopy(_observed_source(source))
    current = source._spec()
    if current == previous:
        return
    _validate_static_source_contract(base, current)

    state = _state_for(scene, source)
    if math.isclose(float(at_time), 0.0, abs_tol=1e-12) and not _object_has_tracks(
        scene, object_id
    ):
        source._retained_timeline_base_spec = copy.deepcopy(current)
        _update_state_from_spec(state, current)
        source._retained_observed_spec = copy.deepcopy(current)
        return

    previous_transform = _retained._transform(previous)
    current_transform = _retained._transform(current)
    previous_position = _retained._vec2(
        previous_transform.get("translation"), "translation"
    )
    current_position = _retained._vec2(
        current_transform.get("translation"), "translation"
    )
    previous_scale = _retained._vec2(previous_transform.get("scale"), "scale")
    current_scale = _retained._vec2(current_transform.get("scale"), "scale")
    previous_rotation = _retained._finite_scalar(
        previous_transform.get("rotation"), "rotation"
    )
    current_rotation = _retained._finite_scalar(
        current_transform.get("rotation"), "rotation"
    )
    previous_opacity = _retained._unit_scalar(previous.get("opacity"), "opacity")
    current_opacity = _retained._unit_scalar(current.get("opacity"), "opacity")
    previous_color = _color(previous)
    current_color = _color(current)

    if not _same_color(previous_color, current_color):
        raise NotImplementedError(
            "direct retained Text color mutation after timeline authoring requires a shared color property channel"
        )

    if not _same_vec2(previous_position, current_position):
        _retained._append_vec2_track(
            scene,
            object_id=object_id,
            property_name="position",
            current=previous_position,
            target=current_position,
            start_time=at_time,
            duration=0.0,
            easing="linear",
        )

    if not _same_vec2(previous_scale, current_scale):
        _retained._append_vec2_track(
            scene,
            object_id=object_id,
            property_name="scale",
            current=previous_scale,
            target=current_scale,
            start_time=at_time,
            duration=0.0,
            easing="linear",
        )

    if not _same_scalar(previous_rotation, current_rotation):
        _retained._append_scalar_track(
            scene,
            object_id=object_id,
            property_name="rotation",
            current=previous_rotation,
            target=current_rotation,
            start_time=at_time,
            duration=0.0,
            easing="linear",
        )

    if not _same_scalar(previous_opacity, current_opacity):
        _retained._append_scalar_track(
            scene,
            object_id=object_id,
            property_name="opacity",
            current=previous_opacity,
            target=current_opacity,
            start_time=at_time,
            duration=0.0,
            easing="linear",
        )

    _update_state_from_spec(state, current)
    source._retained_observed_spec = copy.deepcopy(current)


def _sync_all(scene: _compat.Scene) -> None:
    at_time = float(scene._cursor)
    for source in _bound_sources(scene):
        _sync_source(scene, source, at_time)


def _apply_method_operations(
    source: _typst._RetainedTextMobject,
    operations: list[dict[str, Any]],
) -> None:
    handle = source._retained_handle
    for operation in operations:
        kind = operation["kind"]
        if kind == "scale_relative":
            handle.scale(float(operation["factor"]))
        elif kind == "shift_relative":
            delta = operation["delta"]
            handle.shift(float(delta["x"]), float(delta["y"]))
        elif kind == "move_to":
            position = operation["position"]
            handle.moveTo(float(position["x"]), float(position["y"]))
        elif kind == "set_x":
            position = _retained._vec2(
                _retained._transform(source._spec()).get("translation"), "translation"
            )
            handle.moveTo(float(operation["x"]), float(position["y"]))
        elif kind == "set_y":
            position = _retained._vec2(
                _retained._transform(source._spec()).get("translation"), "translation"
            )
            handle.moveTo(float(position["x"]), float(operation["y"]))
        elif kind == "rotate_relative":
            handle.rotate(float(operation["angle"]))
        elif kind == "opacity_to":
            handle.setOpacity(float(operation["opacity"]))
        else:
            raise ValueError(f"unknown retained animation operation {kind!r}")


def _commit_play_operations(animations: tuple[object, ...]) -> None:
    for animation in animations:
        source = getattr(animation, "source", None)
        operations = vars(animation).get("_retained_method_operations")
        if not isinstance(source, _typst._RetainedTextMobject) or not isinstance(
            operations, list
        ):
            continue
        _apply_method_operations(source, operations)
        _observe_source(source)


def _scene_add(self: _compat.Scene, *mobjects: object, **kwargs: Any):
    _sync_all(self)
    result = _ORIGINAL_SCENE_ADD(self, *mobjects, **kwargs)
    _freeze_bound_sources(self)
    return result


def _scene_remove(self: _compat.Scene, *mobjects: object):
    _sync_all(self)
    return _ORIGINAL_SCENE_REMOVE(self, *mobjects)


def _scene_play(self: _compat.Scene, *animations: Any, **kwargs: Any):
    _sync_all(self)
    result = _ORIGINAL_SCENE_PLAY(self, *animations, **kwargs)
    _freeze_bound_sources(self)
    _commit_play_operations(animations)
    return result


def _scene_wait(self: _compat.Scene, duration: float = 1.0):
    _sync_all(self)
    return _ORIGINAL_SCENE_WAIT(self, duration)


def _retained_document(self: _compat.Scene) -> dict[str, Any]:
    _sync_all(self)
    _freeze_bound_sources(self)
    document = _ORIGINAL_RETAINED_DOCUMENT(self)
    by_id = {int(source.id): source for source in _bound_sources(self)}
    for entry in document.get("objects", []):
        source = by_id.get(int(entry["object"]))
        if source is not None:
            entry["text"] = copy.deepcopy(_freeze_base(source))
    return document


def install() -> None:
    """Install canonical retained authoring-state reconciliation around the scheduler."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _compat.Scene.add = _scene_add
    _compat.Scene.remove = _scene_remove
    _compat.Scene.play = _scene_play
    _compat.Scene.wait = _scene_wait
    _compat.Scene.retained_document = _retained_document
