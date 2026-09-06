"""Deterministic Manim-compatible growing animations.

The supported subset mirrors ManimCE v0.21 ``GrowFromPoint``, ``GrowFromCenter``,
``GrowFromEdge``, and ``SpinInFromNothing`` by lowering the collapsed starting copy
and final mobject into Noon's ordinary transform + lifecycle tracks. No Python
callback runs during playback, and nested composition continues to use the shared
composition scheduler.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_composition as _composition
import _manim_lifecycle as _lifecycle
import _manim_phase_b as _phase_b


_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_COMPOSITION_PLAY_LEAF = _composition._play_leaf
_ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE = _composition._record_composition_wrapper_state
_INSTALLED = False


def _point(value: object) -> _base.Vec2:
    if isinstance(value, (_base.Mobject, _compat.Group)):
        return value.get_center()
    return _compat._as_vec2(value)


class GrowFromPoint:
    """Introduce one leaf mobject by growing it from an exact scene point."""

    def __init__(
        self,
        mobject: object,
        point: object,
        point_color: object | None = None,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "GrowFromPoint currently supports one leaf 2D Mobject; retained groups remain partial"
            )
        self.mobject = mobject
        self.target = mobject
        self.point = _point(point)
        self.point_color = None if point_color is None else _phase_b._as_color(
            "point_color", point_color
        )
        self.anim_args = dict(kwargs)


class GrowFromCenter(GrowFromPoint):
    """Introduce one leaf mobject by growing it from its construction-time center."""

    def __init__(
        self,
        mobject: object,
        point_color: object | None = None,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "GrowFromCenter currently supports one leaf 2D Mobject; retained groups remain partial"
            )
        super().__init__(
            mobject,
            mobject.get_center(),
            point_color=point_color,
            **kwargs,
        )


class GrowFromEdge(GrowFromPoint):
    """Introduce one leaf mobject from the requested construction-time critical point."""

    def __init__(
        self,
        mobject: object,
        edge: object,
        point_color: object | None = None,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, _base.Mobject) or isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "GrowFromEdge currently supports one leaf 2D Mobject; retained groups remain partial"
            )
        direction = _compat._as_vec2(edge)
        super().__init__(
            mobject,
            mobject.get_critical_point(direction),
            point_color=point_color,
            **kwargs,
        )
        self.edge = direction


class SpinInFromNothing(GrowFromCenter):
    """Grow one centered leaf while following Manim's exact spiral path."""

    def __init__(
        self,
        mobject: object,
        angle: float = math.pi / 2.0,
        point_color: object | None = None,
        **kwargs: Any,
    ) -> None:
        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("SpinInFromNothing angle must be finite")
        self.angle = value
        super().__init__(mobject, point_color=point_color, **kwargs)


def _starting_snapshot(animation: GrowFromPoint, final_snapshot: dict[str, Any]) -> dict[str, Any]:
    starting = _animate._snapshot_mobject(final_snapshot)
    starting.scale(0.0)
    starting.move_to(animation.point)
    snapshot = starting.to_ir()

    if isinstance(animation, SpinInFromNothing):
        # Manim's spiral_path(theta) for a GrowFromCenter collapsed source is
        #   c + alpha * R((alpha - 1) * theta) * (p - c).
        # Noon's transform interpolation produces the identical path when the
        # collapsed snapshot starts theta radians behind the final orientation:
        # scale 0->1 and rotation (r-theta)->r share the same eased alpha.
        snapshot["transform"]["rotation"] = (
            float(final_snapshot["transform"]["rotation"]) - animation.angle
        )

    if animation.point_color is not None:
        parsed = animation.point_color
        for channel in ("fill", "stroke"):
            current = snapshot["style"][channel]
            if current is None:
                continue
            alpha = float(current["alpha"])
            snapshot["style"][channel] = parsed.to_ir()
            snapshot["style"][channel]["alpha"] = alpha
    return snapshot


def _schedule_grow(
    scene: _compat.Scene,
    animation: GrowFromPoint,
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    start = float(start_time)
    run_time = float(duration)
    if not math.isfinite(start) or start < 0.0:
        raise ValueError("start_time must be finite and non-negative")
    if not math.isfinite(run_time) or run_time <= 0.0:
        raise ValueError("GrowFromPoint run_time must be finite and positive")

    member = animation.mobject
    plan = _lifecycle._resolve_wrapper(
        scene,
        member,
        "introduce",
        start,
        "growing target",
    )
    if plan.bind:
        _phase_b._bind_raw(scene, member)
    assert member._object is not None
    obj = member._object

    previous_end = scene._scheduled_transform_ends.get(obj.id)
    if previous_end is not None and start < previous_end:
        raise ValueError("generic Transform tracks for one object must not overlap")

    final_snapshot = scene._snapshot_for_object_at(obj, start)
    from_snapshot = _starting_snapshot(animation, final_snapshot)
    object_key = scene._object_keys[obj.id]
    root_key = f"@grow-from-point:{object_key}:{start:g}"

    if plan.show_at_start:
        scene._add_presence_track(
            obj,
            False,
            True,
            start,
            key=f"{root_key}.show",
        )

    scene._add_track(
        obj,
        "transform",
        {
            "object": {
                "from": copy.deepcopy(from_snapshot),
                "to": copy.deepcopy(final_snapshot),
            }
        },
        start,
        run_time,
        easing,
        root_key,
    )
    scene._scheduled_transform_targets[obj.id] = copy.deepcopy(final_snapshot)
    scene._scheduled_transform_ends[obj.id] = start + run_time
    scene._register_top_level(member)


def _resolved_leaf(
    animation: GrowFromPoint,
    *,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
):
    return _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=0.0,
        play_run_time=play_run_time,
        play_easing=play_easing,
        play_rate_func=play_rate_func,
        play_lag_ratio=play_lag_ratio,
    )


def _growing_scene_play(
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
    if getattr(self, "_canonical_authoring_context", None) is not None and not getattr(
        self, "_export_document_construct", False
    ):
        import _manim_canonical_scene as _canonical_scene

        return _canonical_scene._play(
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
    grows = [animation for animation in animations if isinstance(animation, GrowFromPoint)]
    if not grows:
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
    if len(grows) != len(animations):
        raise NotImplementedError(
            "mixing top-level GrowFrom* with unrelated animations in one Scene.play remains partial; use AnimationGroup for deterministic composition"
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
    for animation in grows:
        _animate._record_wrapper_state(animation.mobject, wrapper_states)

    max_end = base_start
    try:
        for animation in grows:
            resolved = _resolved_leaf(
                animation,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule_grow(
                self,
                animation,
                start_time=base_start,
                duration=resolved.run_time,
                easing=resolved.rate_func,
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


def _composition_play_leaf(
    scene: _compat.Scene,
    animation: object,
    *,
    start_time: float,
    run_time: float,
    time_map_steps: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]],
) -> None:
    if not isinstance(animation, GrowFromPoint):
        _ORIGINAL_COMPOSITION_PLAY_LEAF(
            scene,
            animation,
            start_time=start_time,
            run_time=run_time,
            time_map_steps=time_map_steps,
            pending_time_maps=pending_time_maps,
        )
        return

    resolved = _resolved_leaf(
        animation,
        play_run_time=run_time,
        play_easing=None,
        play_rate_func=None,
        play_lag_ratio=None,
    )
    track_start = len(scene._tracks)
    _schedule_grow(
        scene,
        animation,
        start_time=start_time,
        duration=run_time,
        easing=resolved.rate_func,
    )
    track_end = len(scene._tracks)
    if track_end > track_start and _composition._path_requires_time_map(time_map_steps):
        pending_time_maps.append(
            (track_start, track_end, copy.deepcopy(time_map_steps))
        )


def _record_composition_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    if isinstance(animation, GrowFromPoint):
        _animate._record_wrapper_state(animation.mobject, states)
        return
    _ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE(animation, states)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    public = {
        "GrowFromPoint": GrowFromPoint,
        "GrowFromCenter": GrowFromCenter,
        "GrowFromEdge": GrowFromEdge,
        "SpinInFromNothing": SpinInFromNothing,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)

    _compat.Scene.play = _growing_scene_play
    _composition._play_leaf = _composition_play_leaf
    _composition._record_composition_wrapper_state = _record_composition_wrapper_state


install()
