"""Broader source-compatible ``Rotating`` support for Manim documentation scenes.

The existing ``_manim_rotate`` module remains the exact qualified implementation for a
single centered 2D leaf. This layer adds the family/external-pivot/non-z-axis breadth
needed to execute ManimCE v0.21's literal ``RotatingDemo`` without rewriting that
example. Those broader paths stay parity candidates until their intermediate-frame 3D
projection/curved-pivot behavior has dedicated runtime representation and raster gates.
"""

from __future__ import annotations

import copy
import math
from typing import Any

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_rate_functions as _rate_functions
import _manim_rotate as _rotate


_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_INSTALLED = False


class Rotating:
    """Source-compatible ManimCE ``Rotating`` including retained 2D families."""

    def __init__(
        self,
        mobject: object,
        angle: float = math.tau,
        axis: object = _compat.OUT,
        about_point: object | None = None,
        about_edge: object | None = None,
        run_time: float = 5.0,
        rate_func: object = _rate_functions.linear,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Rotating target must be a Mobject or Group")
        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("Rotating angle must be finite")
        self.mobject = mobject
        self.angle = value
        self.axis = axis
        self.about_point = about_point
        self.about_edge = about_edge
        self.anim_args = dict(kwargs)
        self.anim_args["run_time"] = run_time
        self.anim_args["rate_func"] = rate_func


def _axis_kind(axis: object) -> tuple[str, float]:
    try:
        length = len(axis)  # type: ignore[arg-type]
    except (TypeError, AttributeError) as error:
        raise TypeError("rotation axis must be a vector") from error

    if length == 2:
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        if math.isclose(y, 0.0, abs_tol=1e-12) and not math.isclose(x, 0.0, abs_tol=1e-12):
            return "x", 1.0 if x > 0.0 else -1.0
        if math.isclose(x, 0.0, abs_tol=1e-12) and not math.isclose(y, 0.0, abs_tol=1e-12):
            return "y", 1.0 if y > 0.0 else -1.0
        raise NotImplementedError("2D axis constants must be parallel to RIGHT or UP")

    if length == 3:
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        z = float(axis[2])  # type: ignore[index]
        nonzero = [not math.isclose(value, 0.0, abs_tol=1e-12) for value in (x, y, z)]
        if sum(nonzero) != 1:
            raise NotImplementedError("Rotating breadth currently supports principal axes only")
        if nonzero[2]:
            return "z", 1.0 if z > 0.0 else -1.0
        if nonzero[0]:
            return "x", 1.0 if x > 0.0 else -1.0
        return "y", 1.0 if y > 0.0 else -1.0

    raise TypeError("rotation axis must have two or three components")


def _current_detached_members(
    scene: _compat.Scene, value: object, start_time: float
) -> tuple[list[_base.Mobject], list[_base.Mobject]]:
    sources = _compat._leaf_mobjects(value)
    detached: list[_base.Mobject] = []
    for source in sources:
        if source._scene is not scene or source._object is None:
            raise ValueError("Rotating target must belong to this Scene")
        snapshot = scene._snapshot_for_object_at(source._object, start_time)
        detached.append(_animate._snapshot_mobject(snapshot))
    return sources, detached


def _pivot(
    detached: list[_base.Mobject], animation: Rotating
) -> _base.Vec2:
    group = _compat.Group(*detached)
    if animation.about_point is not None:
        return _compat._as_vec2(animation.about_point)
    if animation.about_edge is not None:
        return _compat._critical_for(group, _compat._as_vec2(animation.about_edge))
    return group.get_center()


def _reflect_snapshot(
    snapshot: dict[str, Any], *, axis: str, pivot: _base.Vec2
) -> dict[str, Any]:
    target = copy.deepcopy(snapshot)
    transform = target["transform"]
    translation = transform["translation"]
    rotation = float(transform["rotation"])
    scale = transform["scale"]

    if axis == "y":
        translation["x"] = 2.0 * pivot.x - float(translation["x"])
        transform["rotation"] = math.pi - rotation
        scale["y"] = -float(scale["y"])
    elif axis == "x":
        translation["y"] = 2.0 * pivot.y - float(translation["y"])
        transform["rotation"] = -rotation
        scale["y"] = -float(scale["y"])
    else:
        raise ValueError(f"unsupported reflection axis {axis!r}")
    return target


def _schedule_candidate_family_rotation(
    scene: _compat.Scene,
    animation: Rotating,
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    sources, detached = _current_detached_members(scene, animation.mobject, start_time)
    pivot = _pivot(detached, animation)
    axis, sign = _axis_kind(animation.axis)

    if axis in {"x", "y"} and not math.isclose(
        abs(animation.angle), math.pi, rel_tol=0.0, abs_tol=1e-12
    ):
        raise NotImplementedError(
            "non-z Rotating currently supports the 180-degree projection used by Manim RotatingDemo"
        )

    for index, (source, current) in enumerate(zip(sources, detached, strict=True)):
        assert source._object is not None
        obj = source._object
        snapshot = current.to_ir()
        previous_end = scene._scheduled_transform_ends.get(obj.id)
        if previous_end is not None and start_time < previous_end:
            raise ValueError("Rotating family transforms for one object must not overlap")

        if axis == "z":
            target = _animate._snapshot_mobject(snapshot)
            target.rotate(sign * animation.angle, _compat.OUT, about_point=pivot)
            target_snapshot = target.to_ir()
        else:
            target_snapshot = _reflect_snapshot(snapshot, axis=axis, pivot=pivot)

        object_key = scene._object_keys[obj.id]
        scene._add_track(
            obj,
            "transform",
            {
                "object": {
                    "from": copy.deepcopy(snapshot),
                    "to": copy.deepcopy(target_snapshot),
                }
            },
            start_time,
            duration,
            easing,
            f"@rotating-family:{object_key}:{start_time:g}:{index}",
        )
        scene._scheduled_transform_targets[obj.id] = copy.deepcopy(target_snapshot)
        scene._scheduled_transform_ends[obj.id] = start_time + duration


def _schedule(
    scene: _compat.Scene,
    animation: Rotating,
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    if not isinstance(animation.mobject, _compat.Group):
        exact = _rotate.Rotating(
            animation.mobject,
            angle=animation.angle,
            axis=animation.axis,
            about_point=animation.about_point,
            about_edge=animation.about_edge,
            run_time=duration,
            rate_func=_rate_functions.linear,
        )
        _rotate._schedule_rotate(
            scene,
            exact,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )
        return
    _schedule_candidate_family_rotation(
        scene,
        animation,
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
    broad = [animation for animation in animations if isinstance(animation, Rotating)]
    if not broad:
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
    if len(broad) != len(animations):
        raise NotImplementedError(
            "mixing broad Rotating with unrelated top-level animations remains a parity candidate"
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
    for animation in broad:
        _animate._record_wrapper_state(animation.mobject, wrapper_states)

    max_end = base_start
    try:
        for animation in broad:
            _animate._bind_for_animation(self, animation.mobject, start_time=base_start)
            resolved = _options.resolve(
                builder_args=_options.builder_args(animation),
                default_lag_ratio=0.0,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule(
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


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    for module in (_base, _compat, _animate):
        setattr(module, "Rotating", Rotating)
    if "Rotating" not in _base.__all__:
        _base.__all__.append("Rotating")
    _compat.Scene.play = _scene_play


install()
