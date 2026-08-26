"""ManimCE-compatible procedural ``Rotate`` animation for the exact 2D subset.

``mobject.animate.rotate`` is target-state interpolation and intentionally remains a
regular Transform.  Manim's explicit ``Rotate`` instead follows a rotational path.
For geometry centered on its authored transform origin, Noon can represent that path
exactly with the existing scalar rotation track, without adding a new IR primitive.
External pivots and non-z axes require curved translation/3D support and are rejected
until the runtime can represent them exactly.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_BUILDER_SOURCE = _animate._builder_source
_UNSUPPORTED_PATH_OPTIONS = {
    "path_arc",
    "path_arc_axis",
    "path_arc_centers",
    "path_func",
}


class Rotate:
    """Rotate one centered 2D mobject along Manim's procedural angular path."""

    def __init__(
        self,
        mobject: object,
        angle: float = math.pi,
        axis: object = _compat.OUT,
        about_point: object | None = None,
        about_edge: object | None = None,
        **kwargs: Any,
    ) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "Rotate(Group/VGroup) requires retained family pivot motion and is not yet supported"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("Rotate target must be a Mobject")

        path_options = sorted(_UNSUPPORTED_PATH_OPTIONS.intersection(kwargs))
        if path_options:
            raise NotImplementedError(
                "Rotate path override(s) are not yet supported: " + ", ".join(path_options)
            )

        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("Rotate angle must be finite")

        self.mobject = mobject
        self.angle = value
        self.axis = axis
        # ManimCE eagerly captures the default pivot in Rotate.__init__; do the same
        # rather than recomputing the center later when Scene.play is called.
        self.about_point = mobject.get_center() if about_point is None else about_point
        self.about_edge = about_edge
        self.anim_args = dict(kwargs)


def _axis_sign(axis: object) -> float:
    try:
        if len(axis) != 3:  # type: ignore[arg-type]
            raise NotImplementedError("Rotate currently supports only 3D z-axis vectors")
        x = float(axis[0])  # type: ignore[index]
        y = float(axis[1])  # type: ignore[index]
        z = float(axis[2])  # type: ignore[index]
    except NotImplementedError:
        raise
    except (TypeError, ValueError, IndexError) as error:
        raise TypeError("Rotate axis must be a three-component numeric vector") from error

    if not all(math.isfinite(value) for value in (x, y, z)):
        raise ValueError("Rotate axis must be finite")
    if not math.isclose(x, 0.0, abs_tol=1e-12) or not math.isclose(
        y, 0.0, abs_tol=1e-12
    ) or math.isclose(z, 0.0, abs_tol=1e-12):
        raise NotImplementedError("Rotate currently supports only the 2D OUT/IN z axis")
    return 1.0 if z > 0.0 else -1.0


def _points_close(left: _base.Vec2, right: _base.Vec2) -> bool:
    return math.isclose(left.x, right.x, abs_tol=1e-9) and math.isclose(
        left.y, right.y, abs_tol=1e-9
    )


def _validate_exact_pivot(
    scene: _compat.Scene,
    animation: Rotate,
    *,
    start_time: float,
) -> tuple[object, dict[str, Any]]:
    mobject = animation.mobject
    if mobject._scene is not scene or mobject._object is None:
        raise ValueError("Rotate target must belong to this Scene")
    obj = mobject._object
    snapshot = scene._snapshot_for_object_at(obj, start_time)
    detached = _animate._snapshot_mobject(snapshot)
    center = detached.get_center()
    translation = snapshot["transform"]["translation"]
    transform_origin = _base.Vec2(
        float(translation["x"]),
        float(translation["y"]),
    )

    # A scalar Noon rotation is around the object's transform origin.  For centered
    # analytic geometry (including the quickstart Square), that is exactly Manim's
    # default Rotate pivot.  Offset local geometry would need a circular translation
    # path to keep its center fixed, so do not approximate it with linear motion.
    if not _points_close(center, transform_origin):
        raise NotImplementedError(
            "Rotate currently requires geometry centered on its transform origin"
        )

    about_point = _compat._as_vec2(animation.about_point)
    if not _points_close(about_point, center):
        raise NotImplementedError(
            "Rotate about an external point requires curved translation and is not yet supported"
        )

    # Manim Rotate eagerly defaults about_point to mobject.get_center(); Mobject.rotate
    # gives about_point precedence over about_edge.  Therefore about_edge does not
    # alter the supported default/centered Rotate path and can be accepted unchanged.
    return obj, snapshot


def _ensure_rotation_interval_available(
    scene: _compat.Scene,
    obj: object,
    *,
    start_time: float,
    duration: float,
) -> None:
    end_time = start_time + duration
    object_id = obj.id  # type: ignore[attr-defined]
    for track in scene._tracks:
        if track["object"] != object_id or track["property"] not in {
            "rotation",
            "transform",
        }:
            continue
        track_start = float(track["timing"]["start_time"])
        track_end = track_start + float(track["timing"]["duration"])
        if track_start < end_time and start_time < track_end:
            raise ValueError(
                "Rotate cannot overlap another rotation/generic Transform on the same object"
            )


def _schedule_rotate(
    scene: _compat.Scene,
    animation: Rotate,
    *,
    start_time: float,
    duration: float,
    easing: str,
) -> None:
    obj, snapshot = _validate_exact_pivot(scene, animation, start_time=start_time)
    _ensure_rotation_interval_available(
        scene,
        obj,
        start_time=start_time,
        duration=duration,
    )

    from_rotation = float(snapshot["transform"]["rotation"])
    to_rotation = from_rotation + _axis_sign(animation.axis) * animation.angle
    object_key = scene._object_keys[obj.id]
    scene._add_scalar_track(
        obj,
        "rotation",
        from_rotation,
        to_rotation,
        start_time,
        duration,
        easing,
        f"@rotate:{object_key}:{start_time:g}",
    )


def _builder_source(animation: object) -> object | None:
    if isinstance(animation, Rotate):
        return animation.mobject
    return _ORIGINAL_BUILDER_SOURCE(animation)


def _rotate_scene_play(
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
    if not any(isinstance(animation, Rotate) for animation in animations):
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
    for animation in animations:
        source = _builder_source(animation)
        if source is not None:
            _animate._record_wrapper_state(source, wrapper_states)
        if isinstance(animation, (_base.Create, _base.FadeIn, _base.FadeOut)):
            _animate._record_wrapper_state(animation.target, wrapper_states)

    max_end = base_start
    try:
        for animation in animations:
            if isinstance(animation, Rotate):
                _animate._bind_for_animation(
                    self,
                    animation.mobject,
                    start_time=base_start,
                )
                resolved = _options.resolve(
                    builder_args=_options.builder_args(animation),
                    default_lag_ratio=0.0,
                    play_run_time=play_run_time,
                    play_easing=easing,
                    play_rate_func=rate_func,
                    play_lag_ratio=lag_ratio,
                )
                _schedule_rotate(
                    self,
                    animation,
                    start_time=base_start,
                    duration=resolved.run_time,
                    easing=resolved.rate_func,
                )
                end = base_start + resolved.run_time
            else:
                _ORIGINAL_SCENE_PLAY(
                    self,
                    animation,
                    run_time=play_run_time,
                    start_time=base_start,
                    easing=easing,
                    rate_func=rate_func,
                    lag_ratio=lag_ratio,
                )
                end = self._cursor
            max_end = max(max_end, end)

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

    public = {"Rotate": Rotate}
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        setattr(_animate, name, value)

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports

    # Composition imports this module before capturing Scene.play, so nested Rotate
    # leaves share the same timing resolver and rollback path as top-level plays.
    _animate._builder_source = _builder_source
    _compat.Scene.play = _rotate_scene_play
