"""Batch retained Group/VGroup fades across one source-level animation.

The ordinary retained Text scheduler remains the sole owner of leaf lifecycle and
property-track semantics. This adapter classifies a source-level retained family fade
into ordered leaves, then lets the existing leaf scheduler consume each leaf under the
same retained batch plan for standalone and mixed family/property plays. It never
materializes per-leaf public FadeIn/FadeOut animations.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_retained_animate as _retained
import _manim_typst as _typst


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = None
_ORIGINAL_ANIMATION_PLAN = None
_ORIGINAL_ANIMATION_SOURCE = None
_ORIGINAL_SCHEDULE_PLAN = None


class _RetainedFamilyFadeBatch(list[dict[str, Any]]):
    """One source animation plus the ordered retained leaves it owns."""

    def __init__(
        self,
        family: _compat.Group,
        leaves: list[_typst._RetainedTextMobject],
        operation: dict[str, Any],
    ) -> None:
        super().__init__([copy.deepcopy(operation)])
        self.family = family
        self.leaves = tuple(leaves)


class _RetainedBatchLeafAnimation:
    """Internal source carrier for the existing retained leaf scheduler."""

    def __init__(self, source: _typst._RetainedTextMobject) -> None:
        self.source = source


def _family_fade_batch(animation: object) -> _RetainedFamilyFadeBatch | None:
    if not isinstance(animation, (_animate.FadeIn, _animate.FadeOut)):
        return None
    target = getattr(animation, "target", None)
    if not isinstance(target, _compat.Group):
        return None

    leaves = _compat._leaf_mobjects(target)
    retained = [
        member for member in leaves if isinstance(member, _typst._RetainedTextMobject)
    ]
    if not retained:
        return None
    if len(retained) != len(leaves):
        raise NotImplementedError(
            "mixing retained Text and legacy Mobjects in one Group/VGroup fade is not supported"
        )

    shift = _compat._as_vec2(_animate._legacy_fade_shift_vector(animation))
    scale_factor = float(animation._fade_scale_factor)
    if not math.isfinite(scale_factor):
        raise ValueError("retained text family fade scale must be finite")
    default_endpoint = (
        math.isclose(float(shift.x), 0.0, abs_tol=1e-15)
        and math.isclose(float(shift.y), 0.0, abs_tol=1e-15)
        and math.isclose(scale_factor, 1.0, rel_tol=0.0, abs_tol=1e-15)
        and not bool(animation._fade_point_target)
    )
    if not default_endpoint:
        raise NotImplementedError(
            "retained Text Group/VGroup fades with shift, scale, or target_position "
            "require shared retained family layout semantics"
        )

    animation_args = _options.builder_args(animation)
    family_lag_ratio = float(animation_args.get("lag_ratio", 0.0))
    if not math.isfinite(family_lag_ratio):
        raise ValueError("retained text family fade lag_ratio must be finite")
    if not math.isclose(family_lag_ratio, 0.0, rel_tol=0.0, abs_tol=1e-15):
        raise NotImplementedError(
            "retained Text Group/VGroup fade lag_ratio requires shared retained family scheduling"
        )

    operation = {
        "kind": "fade_in" if isinstance(animation, _animate.FadeIn) else "fade_out",
        "shift": {"x": float(shift.x), "y": float(shift.y)},
        "scale_factor": scale_factor,
        "point_target": bool(animation._fade_point_target),
    }
    return _RetainedFamilyFadeBatch(target, retained, operation)


def _standalone_batch_passthrough(
    animation: object,
) -> tuple[_compat.Group, list[_typst._RetainedTextMobject], list[object]] | None:
    """Keep the source animation intact while the retained Scene.play hook classifies its family."""

    batch = _family_fade_batch(animation)
    if batch is None:
        return None
    return batch.family, list(batch.leaves), [animation]


def _animation_plan(animation: object) -> list[dict[str, Any]] | None:
    batch = _family_fade_batch(animation)
    if batch is not None:
        return batch
    assert _ORIGINAL_ANIMATION_PLAN is not None
    return _ORIGINAL_ANIMATION_PLAN(animation)


def _animation_source(animation: object) -> _typst._RetainedTextMobject | None:
    if isinstance(animation, _RetainedBatchLeafAnimation):
        return animation.source
    batch = _family_fade_batch(animation)
    if batch is not None:
        # The existing family transaction snapshots one retained source itself. The
        # outer batch wrapper snapshots every leaf, so returning the first leaf keeps
        # the legacy single-source contract valid without losing rollback coverage.
        return batch.leaves[0]
    assert _ORIGINAL_ANIMATION_SOURCE is not None
    return _ORIGINAL_ANIMATION_SOURCE(animation)


def _schedule_plan(
    scene: _compat.Scene,
    animation: object,
    operations: list[dict[str, Any]],
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    assert _ORIGINAL_SCHEDULE_PLAN is not None
    if not isinstance(operations, _RetainedFamilyFadeBatch):
        _ORIGINAL_SCHEDULE_PLAN(
            scene,
            animation,
            operations,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
        return

    leaf_operations = [copy.deepcopy(operations[0])]
    for source in operations.leaves:
        _ORIGINAL_SCHEDULE_PLAN(
            scene,
            _RetainedBatchLeafAnimation(source),
            leaf_operations,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )


def _scene_play(
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
    batches = [
        batch for animation in animations if (batch := _family_fade_batch(animation)) is not None
    ]
    if not batches:
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

    if lag_ratio is not None:
        play_lag_ratio = float(lag_ratio)
        if not math.isfinite(play_lag_ratio):
            raise ValueError("retained text family fade lag_ratio must be finite")
        if not math.isclose(play_lag_ratio, 0.0, rel_tol=0.0, abs_tol=1e-15):
            raise NotImplementedError(
                "retained Text Group/VGroup Scene.play lag_ratio requires shared retained family scheduling"
            )

    top_level_before = list(self._compat_top_level)
    wrapper_states = {
        id(source): (
            source,
            source._scene,
            source._object,
            source._retained_object_id,
            source._retained_order,
        )
        for batch in batches
        for source in batch.leaves
    }

    assert _ORIGINAL_SCENE_PLAY is not None
    try:
        result = _ORIGINAL_SCENE_PLAY(
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
        _retained._normalize_retained_family_top_level(
            self,
            [(batch.family, list(batch.leaves)) for batch in batches],
            top_level_before,
        )
        return result
    except Exception:
        self._compat_top_level = top_level_before
        for source, old_scene, old_object, old_object_id, old_order in wrapper_states.values():
            source._scene = old_scene
            source._object = old_object
            source._retained_object_id = old_object_id
            source._retained_order = old_order
        raise


def install() -> None:
    """Install above canonical family creation after its outer transaction exists."""

    global _INSTALLED
    global _ORIGINAL_SCENE_PLAY
    global _ORIGINAL_ANIMATION_PLAN
    global _ORIGINAL_ANIMATION_SOURCE
    global _ORIGINAL_SCHEDULE_PLAN

    if _INSTALLED:
        return
    _INSTALLED = True

    _ORIGINAL_ANIMATION_PLAN = _retained._retained_animation_plan
    _ORIGINAL_ANIMATION_SOURCE = _retained._retained_animation_source
    _ORIGINAL_SCHEDULE_PLAN = _retained._schedule_retained_plan
    _ORIGINAL_SCENE_PLAY = _compat.Scene.play

    _retained._retained_family_fade_expansion = _standalone_batch_passthrough
    _retained._retained_animation_plan = _animation_plan
    _retained._retained_animation_source = _animation_source
    _retained._schedule_retained_plan = _schedule_plan
    _compat.Scene.play = _scene_play
