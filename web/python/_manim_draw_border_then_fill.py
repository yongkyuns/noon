"""Deterministic ManimCE v0.21 ``DrawBorderThenFill`` compatibility.

The exact default behavior is lowered to Noon's existing retained timeline:

* phase 1 reveals an outline snapshot with local ``smooth`` easing;
* phase 2 transforms that outline snapshot into the final styled object with local
  ``smooth`` easing.

Manim's default ``double_smooth`` rate function maps exactly to those two equal
half-duration phases. No Python callback runs during playback.
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


def _double_smooth(t: float) -> float:
    """Pinned ManimCE v0.21 double_smooth, used only as the constructor sentinel."""

    value = float(t)
    if value < 0.5:
        return 0.5 * _compat.smooth(2.0 * value)
    return 0.5 * (1.0 + _compat.smooth(2.0 * value - 1.0))


# Keep the upstream callable name for constructor/introspection parity without adding a
# new runtime easing ID. This adapter decomposes it exactly before playback.
_double_smooth.__name__ = "double_smooth"


def _is_double_smooth(value: object) -> bool:
    return value is _double_smooth or getattr(value, "__name__", None) == "double_smooth"


class DrawBorderThenFill:
    """Draw a VMobject's outline, then interpolate the outline into its final style."""

    def __init__(
        self,
        vmobject: object,
        run_time: float = 2.0,
        rate_func: object = _double_smooth,
        stroke_width: float = 2.0,
        stroke_color: object | None = None,
        introducer: bool = True,
        **kwargs: Any,
    ) -> None:
        if not isinstance(vmobject, _compat.VMobject) or isinstance(vmobject, _compat.Group):
            raise TypeError("DrawBorderThenFill only works for one leaf VMobject in this parity slice")
        if not _is_double_smooth(rate_func):
            raise NotImplementedError(
                "DrawBorderThenFill currently supports Manim's default double_smooth rate_func only"
            )
        width = float(stroke_width)
        if not math.isfinite(width) or width < 0.0:
            raise ValueError("stroke_width must be finite and non-negative")

        self.mobject = vmobject
        self.target = vmobject
        self.stroke_width = width
        self.stroke_color = (
            None
            if stroke_color is None
            else _phase_b._as_color("stroke_color", stroke_color)
        )
        self.introducer = bool(introducer)
        self.anim_args = dict(kwargs)
        self.anim_args["run_time"] = float(run_time)


def _outline_snapshot(
    animation: DrawBorderThenFill,
    final_snapshot: dict[str, Any],
) -> dict[str, Any]:
    outline = copy.deepcopy(final_snapshot)
    style = outline["style"]

    fill = style.get("fill")
    if fill is not None:
        fill["alpha"] = 0.0

    stroke = style.get("stroke")
    final_width = float(style.get("stroke_width", 0.0))
    if animation.stroke_color is not None:
        alpha = 1.0 if stroke is None else float(stroke.get("alpha", 1.0))
        stroke = animation.stroke_color.to_ir()
        stroke["alpha"] = alpha
        style["stroke"] = stroke
    elif stroke is None or final_width <= 0.0:
        # Manim falls back to ``mob.get_color()`` when no usable stroke is present.
        # For the current leaf VMobject facade, the visible fill color is the closest
        # semantic equivalent; ordinary VMobjects retain a stroke and never use this
        # fallback. Keeping it deterministic is preferable to silently using white.
        fallback = fill
        if fallback is None:
            raise NotImplementedError(
                "DrawBorderThenFill stroke fallback requires a stroke or fill color"
            )
        style["stroke"] = copy.deepcopy(fallback)
        style["stroke"]["alpha"] = 1.0

    style["stroke_width"] = _phase_b._manim_stroke_width(animation.stroke_width)
    return outline


def _add_transform_track(
    scene: _compat.Scene,
    obj: _base._ir.Object,
    from_snapshot: dict[str, Any],
    to_snapshot: dict[str, Any],
    *,
    start_time: float,
    duration: float,
    key: str,
) -> None:
    scene._add_track(
        obj,
        "transform",
        {
            "object": {
                "from": copy.deepcopy(from_snapshot),
                "to": copy.deepcopy(to_snapshot),
            }
        },
        start_time,
        duration,
        "smooth",
        key,
    )


def _schedule_draw_border_then_fill(
    scene: _compat.Scene,
    animation: DrawBorderThenFill,
    *,
    start_time: float,
    duration: float,
) -> None:
    start = float(start_time)
    run_time = float(duration)
    if not math.isfinite(start) or start < 0.0:
        raise ValueError("start_time must be finite and non-negative")
    if not math.isfinite(run_time) or run_time <= 0.0:
        raise ValueError("DrawBorderThenFill run_time must be finite and positive")

    member = animation.mobject
    intent = "introduce" if animation.introducer else "require_present"
    plan = _lifecycle._resolve_wrapper(
        scene,
        member,
        intent,
        start,
        "DrawBorderThenFill target",
    )
    if plan.bind:
        _phase_b._bind_raw(scene, member)
    assert member._object is not None
    obj = member._object

    previous_end = scene._scheduled_transform_ends.get(obj.id)
    if previous_end is not None and start < previous_end:
        raise ValueError("generic Transform tracks for one object must not overlap")

    end = start + run_time
    for track in scene._tracks:
        if track["object"] != obj.id or track["property"] != "reveal":
            continue
        track_start = float(track["timing"]["start_time"])
        track_end = track_start + float(track["timing"]["duration"])
        if track_start < end and start < track_end:
            raise ValueError("DrawBorderThenFill reveal must not overlap another reveal")

    final_snapshot = scene._snapshot_for_object_at(obj, start)
    geometry = final_snapshot["geometry"]
    if not any(name in geometry for name in ("circle", "rectangle", "line", "vector_path")):
        raise ValueError("DrawBorderThenFill requires supported vector geometry")
    outline_snapshot = _outline_snapshot(animation, final_snapshot)

    object_key = scene._object_keys[obj.id]
    root_key = f"@draw-border-then-fill:{object_key}:{start:g}"
    half = run_time * 0.5

    if plan.show_at_start:
        scene._add_presence_track(
            obj,
            False,
            True,
            start,
            key=f"{root_key}.show",
        )

    # double_smooth(t) + integer_interpolate(0, 2, alpha) is exactly equivalent
    # to two equal-duration smooth phases. Hold outline style during phase one while
    # the ordinary reveal channel traces its geometry, then interpolate style during
    # phase two. Geometry endpoints are identical, so generic Transform only changes
    # the style in the second phase.
    _add_transform_track(
        scene,
        obj,
        outline_snapshot,
        outline_snapshot,
        start_time=start,
        duration=half,
        key=f"{root_key}.outline",
    )
    scene._add_scalar_track(
        obj,
        "reveal",
        0.0,
        1.0,
        start,
        half,
        "smooth",
        f"{root_key}.reveal",
    )
    _add_transform_track(
        scene,
        obj,
        outline_snapshot,
        final_snapshot,
        start_time=start + half,
        duration=half,
        key=f"{root_key}.fill",
    )

    # Reintroduction after a previous FadeOut must not inherit appearance=0.
    if scene._appearance_at(obj, start) != 1.0:
        scene._add_scalar_track(
            obj,
            "appearance",
            1.0,
            1.0,
            start,
            run_time,
            "linear",
            f"{root_key}.appearance",
        )

    scene._scheduled_transform_targets[obj.id] = copy.deepcopy(final_snapshot)
    scene._scheduled_transform_ends[obj.id] = end
    scene._register_top_level(member)


def _resolved_run_time(
    animation: DrawBorderThenFill,
    *,
    play_run_time: float | None,
    play_lag_ratio: float | None,
) -> float:
    resolved = _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=0.0,
        play_run_time=play_run_time,
        play_easing=None,
        play_rate_func=None,
        play_lag_ratio=play_lag_ratio,
    )
    return resolved.run_time


def _draw_border_then_fill_scene_play(
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
    draws = [animation for animation in animations if isinstance(animation, DrawBorderThenFill)]
    if not draws:
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
    if len(draws) != len(animations):
        raise NotImplementedError(
            "mixing top-level DrawBorderThenFill with unrelated animations remains partial; use AnimationGroup"
        )
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None:
        raise NotImplementedError(
            "DrawBorderThenFill currently supports Manim's default double_smooth rate_func only"
        )
    if rate_func is not None and not _is_double_smooth(rate_func):
        raise NotImplementedError(
            "DrawBorderThenFill currently supports Manim's default double_smooth rate_func only"
        )
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
    for animation in draws:
        _animate._record_wrapper_state(animation.mobject, wrapper_states)

    max_end = base_start
    try:
        for animation in draws:
            actual_run_time = _resolved_run_time(
                animation,
                play_run_time=play_run_time,
                play_lag_ratio=lag_ratio,
            )
            _schedule_draw_border_then_fill(
                self,
                animation,
                start_time=base_start,
                duration=actual_run_time,
            )
            max_end = max(max_end, base_start + actual_run_time)
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
    if not isinstance(animation, DrawBorderThenFill):
        _ORIGINAL_COMPOSITION_PLAY_LEAF(
            scene,
            animation,
            start_time=start_time,
            run_time=run_time,
            time_map_steps=time_map_steps,
            pending_time_maps=pending_time_maps,
        )
        return

    track_start = len(scene._tracks)
    _schedule_draw_border_then_fill(
        scene,
        animation,
        start_time=start_time,
        duration=run_time,
    )
    track_end = len(scene._tracks)
    if track_end <= track_start or not _composition._path_requires_time_map(time_map_steps):
        return

    # The generic composition helper can map a normal leaf by assigning one time map
    # to every emitted track because a normal leaf occupies one continuous interval.
    # DrawBorderThenFill is deliberately split into two retained half-phases, so each
    # animated track needs one extra local interval step before the parent path. This
    # preserves the 0..0.5 outline phase and 0.5..1 fill phase even under nonlinear
    # nested AnimationGroup/LaggedStart rate functions.
    for index in range(track_start, track_end):
        track = scene._tracks[index]
        if track["property"] == "presence":
            continue
        local_start = (float(track["timing"]["start_time"]) - start_time) / run_time
        local_duration = float(track["timing"]["duration"]) / run_time
        steps = copy.deepcopy(time_map_steps)
        if not (
            math.isclose(local_start, 0.0, abs_tol=1e-12)
            and math.isclose(local_duration, 1.0, abs_tol=1e-12)
        ):
            steps.append(
                {
                    "start": local_start,
                    "duration": local_duration,
                    "rate_func": "linear",
                }
            )
        pending_time_maps.append((index, index + 1, steps))


def _record_composition_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    if isinstance(animation, DrawBorderThenFill):
        _animate._record_wrapper_state(animation.mobject, states)
        return
    _ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE(animation, states)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    setattr(_base, "DrawBorderThenFill", DrawBorderThenFill)
    if "DrawBorderThenFill" not in _base.__all__:
        _base.__all__.append("DrawBorderThenFill")

    _compat.Scene.play = _draw_border_then_fill_scene_play
    _composition._play_leaf = _composition_play_leaf
    _composition._record_composition_wrapper_state = _record_composition_wrapper_state


install()
