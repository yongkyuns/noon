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


def _normal_builder_scale_factor(
    animation: _animate._AlignedAnimationBuilder,
    source: _typst._RetainedTextMobject,
) -> float:
    """Extract one uniform relative scale from a normal ``mobject.animate`` builder.

    The generic Manim builder mutates a detached target copy. Retained playback owns
    the semantic current state separately, so the only frontend value we need is the
    source-to-target scale ratio. Applying that ratio to the retained state makes
    sequential calls such as ``scale(2)`` then ``scale(0.5)`` compose correctly without
    mutating the source object or reconstructing a legacy geometry snapshot.
    """

    target = animation.target
    if type(target) is not type(source):
        raise NotImplementedError(
            "retained Text .animate currently requires a target of the same retained text type"
        )

    source_spec = source._spec()
    target_spec = target._spec()
    source_transform = source_spec.get("transform")
    target_transform = target_spec.get("transform")
    if not isinstance(source_transform, dict) or not isinstance(target_transform, dict):
        raise ValueError("retained text authoring spec is missing transform state")

    source_scale = source_transform.get("scale")
    target_scale = target_transform.get("scale")
    if not isinstance(source_scale, dict) or not isinstance(target_scale, dict):
        raise ValueError("retained text authoring spec is missing scale state")

    source_without_scale = copy.deepcopy(source_spec)
    target_without_scale = copy.deepcopy(target_spec)
    source_without_scale["transform"]["scale"] = None
    target_without_scale["transform"]["scale"] = None
    if source_without_scale != target_without_scale:
        raise NotImplementedError(
            "retained Text .animate currently supports uniform scale only; "
            "position, rotation, color, and opacity animation remain separate slices"
        )

    source_x = float(source_scale["x"])
    source_y = float(source_scale["y"])
    target_x = float(target_scale["x"])
    target_y = float(target_scale["y"])
    if not all(math.isfinite(value) for value in (source_x, source_y, target_x, target_y)):
        raise ValueError("retained text scale state must be finite")
    if math.isclose(source_x, 0.0, abs_tol=1e-15) or math.isclose(
        source_y, 0.0, abs_tol=1e-15
    ):
        raise ValueError("retained Text .animate cannot derive scale from a zero-scale source")

    factor_x = target_x / source_x
    factor_y = target_y / source_y
    if not math.isclose(factor_x, factor_y, rel_tol=1e-12, abs_tol=1e-12):
        raise NotImplementedError(
            "retained Text .animate currently supports uniform scale only"
        )
    factor = (factor_x + factor_y) / 2.0
    if not math.isfinite(factor) or factor <= 0.0:
        raise ValueError("retained Text .animate scale factor must be finite and positive")
    return factor


def _retained_scale_factor(animation: object) -> float | None:
    """Return a retained relative scale factor, or ``None`` for another animation."""

    if not isinstance(animation, _animate._AlignedAnimationBuilder):
        return None
    source = getattr(animation, "source", None)
    if not isinstance(source, _typst._RetainedTextMobject):
        return None

    # ScaleInPlace/ShrinkToCenter use a deferred builder because Manim creates their
    # target at play-begin time. Keep that zero-scale-capable path independent of the
    # normal .animate target copy, whose retained ``scale`` mutator intentionally
    # accepts positive factors only.
    deferred_factor = vars(animation).get("scale_factor")
    if deferred_factor is not None:
        factor = float(deferred_factor)
        if not math.isfinite(factor):
            raise ValueError("retained text scale factor must be finite")
        return factor

    return _normal_builder_scale_factor(animation, source)


def _bind_retained(scene: _compat.Scene, source: _typst._RetainedTextMobject) -> None:
    if source._scene is None:
        _typst._add_retained(scene, source, None)
    elif source._scene is not scene:
        raise ValueError("retained text Mobject already belongs to another Scene")
    else:
        scene._register_top_level(source)


def _initial_scale(source: _typst._RetainedTextMobject) -> dict[str, float]:
    scale = source._spec()["transform"]["scale"]
    return {"x": float(scale["x"]), "y": float(scale["y"])}


def _schedule_retained_scale(
    scene: _compat.Scene,
    animation: object,
    factor: float,
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
        object_id, {"scale": _initial_scale(source)}
    )
    current = state["scale"]
    target = {
        "x": float(current["x"]) * factor,
        "y": float(current["y"]) * factor,
    }
    scene._retained_animation_tracks.append(
        {
            "object": object_id,
            "property": "scale",
            "values": {
                "vec2": {
                    "from": {"x": float(current["x"]), "y": float(current["y"])},
                    "to": target,
                }
            },
            "timing": {
                "start_time": float(start_time),
                "duration": float(duration),
                "easing": str(easing),
            },
        }
    )
    state["scale"] = copy.deepcopy(target)


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
    retained_factors = [_retained_scale_factor(animation) for animation in animations]
    if not any(factor is not None for factor in retained_factors):
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
    if any(factor is None for factor in retained_factors):
        raise NotImplementedError(
            "mixing retained Text scale animations with legacy animations in one Scene.play "
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
        for animation, factor in zip(animations, retained_factors):
            assert factor is not None
            resolved = _options.resolve(
                builder_args=_options.builder_args(animation),
                default_lag_ratio=0.0,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule_retained_scale(
                self,
                animation,
                factor,
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
    # The generic semantic-handle target cloner is geometry-backed. Retained Text/Typst
    # instead owns source-level resource state, so its normal `.animate` target must clone
    # through the retained handle before any target-state mutator is applied.
    _typst._RetainedTextMobject._copy_for_animate_target = _retained_copy_for_animate_target
    _compat.Scene.play = _retained_scene_play
    _compat.Scene.retained_document = _retained_document
