"""Deterministic ManimCE v0.21 indication compatibility.

``ShowPassingFlash`` qualifies one retained ``Line`` by lowering Manim's moving
partial-path window to ordinary Position and Reveal tracks. ``Flash`` then reuses
that exact leaf implementation through Noon's existing ``AnimationGroup`` scheduler:
its radial lines are static authoring geometry and no Python callback participates in
playback.

For ``ShowPassingFlash`` Manim's ``ShowPartial`` implementation exposes

    lower = max((1 + time_width) * alpha - time_width, 0)
    upper = min((1 + time_width) * alpha, 1)

where ``alpha`` has already passed through the animation rate function. A line window
can be represented exactly without changing semantic geometry: translate the line by
``lower`` times its transformed direction and reveal a prefix of length
``upper - lower``. Noon authors those two quantities as ordinary retained Position
and Reveal tracks with shared composition time maps, so seeking/playback never wakes
Python.

At the remover boundary constant cleanup tracks restore the original translation and
full reveal before the object is hidden. This mirrors Manim's
``ShowPassingFlash.clean_up_from_scene`` restoration and makes reusing the same Line
after the animation deterministic.
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


_INSTALLED = False
_ORIGINAL_SCENE_PLAY = _compat.Scene.play
_ORIGINAL_COMPOSITION_PLAY_LEAF = _composition._play_leaf
_ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE = _composition._record_composition_wrapper_state
_PURE_YELLOW = _base.color_from_hex("#FFFF00")


class ShowPassingFlash:
    """Show a moving window of one retained 2D Line, then remove it."""

    def __init__(
        self,
        mobject: object,
        time_width: float = 0.1,
        **kwargs: Any,
    ) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "ShowPassingFlash retained family semantics remain partial"
            )
        if not isinstance(mobject, _compat.VMobject):
            raise TypeError("ShowPassingFlash only works for VMobjects")
        raw = mobject._current_raw()
        if "line" not in raw.geometry:
            raise NotImplementedError(
                "ShowPassingFlash currently qualifies the exact Line subset; "
                "general VMobject path windows remain partial"
            )
        width = float(time_width)
        if not math.isfinite(width) or width <= 0.0:
            raise NotImplementedError(
                "ShowPassingFlash currently requires a finite positive time_width"
            )
        if bool(kwargs.get("reverse_rate_function", False)):
            raise NotImplementedError(
                "ShowPassingFlash reverse_rate_function=True remains partial"
            )
        if "path_arc" in kwargs:
            raise TypeError("ShowPassingFlash does not accept path_arc")

        self.mobject = mobject
        self.target = mobject
        self.time_width = width
        self.remover = True
        self.introducer = True
        self.anim_args = dict(kwargs)


class Flash(_composition.AnimationGroup):
    """Send retained passing-flash Lines radially from a fixed 2D point."""

    def __init__(
        self,
        point: object,
        line_length: float = 0.2,
        num_lines: int = 12,
        flash_radius: float = 0.1,
        line_stroke_width: float = 3.0,
        color: object = _PURE_YELLOW,
        time_width: float = 1.0,
        run_time: float = 1.0,
        **kwargs: Any,
    ) -> None:
        if isinstance(point, (_base.Mobject, _compat.Group)):
            center = point.get_center()
        else:
            center = _compat._as_vec2(point)

        if isinstance(num_lines, bool) or not isinstance(num_lines, int):
            raise TypeError("Flash num_lines must be an integer")
        if num_lines <= 0:
            raise ValueError("Flash num_lines must be positive")

        length = float(line_length)
        radius = float(flash_radius)
        stroke_width = float(line_stroke_width)
        duration = float(run_time)
        if not math.isfinite(length):
            raise ValueError("Flash line_length must be finite")
        if not math.isfinite(radius):
            raise ValueError("Flash flash_radius must be finite")
        if not math.isfinite(stroke_width) or stroke_width < 0.0:
            raise ValueError("Flash line_stroke_width must be finite and non-negative")
        if not math.isfinite(duration) or duration <= 0.0:
            raise ValueError("Flash run_time must be finite and positive")

        self.point = center
        self.color = _phase_b._as_color("color", color)
        self.line_length = length
        self.num_lines = num_lines
        self.flash_radius = radius
        self.line_stroke_width = stroke_width
        self.run_time = duration
        self.time_width = float(time_width)
        self.animation_config = dict(kwargs)

        self.lines = self.create_lines()
        animations = self.create_line_anims()
        # Match Manim: timing/rate kwargs belong to each ShowPassingFlash child;
        # the containing AnimationGroup itself keeps its default linear, lag=0 map.
        super().__init__(*animations, group=self.lines, run_time=duration)

    def create_lines(self) -> _compat.VGroup:
        lines = _compat.VGroup()
        for index in range(self.num_lines):
            angle = _base.TAU * index / self.num_lines
            line = _compat.Line(
                self.point,
                self.point + self.line_length * _base.RIGHT,
            )
            line.shift(self.flash_radius * _base.RIGHT)
            line.rotate(angle, about_point=self.point)
            lines.add(line)
        lines.set_color(self.color)
        lines.set_stroke(width=self.line_stroke_width)
        return lines

    def create_line_anims(self) -> list[ShowPassingFlash]:
        return [
            ShowPassingFlash(
                line,
                time_width=self.time_width,
                run_time=self.run_time,
                **self.animation_config,
            )
            for line in self.lines
        ]


def _track_key(scene: _compat.Scene, track: dict[str, Any]) -> str:
    return str(scene._track_keys.get(int(track["id"]), ""))


def _overlaps(timing: dict[str, Any], start: float, end: float) -> bool:
    track_start = float(timing["start_time"])
    track_end = track_start + float(timing["duration"])
    return track_start < end and start < track_end


def _ensure_window_channels_available(
    scene: _compat.Scene,
    obj: _base._ir.Object,
    *,
    start: float,
    end: float,
) -> None:
    for track in scene._tracks:
        if int(track["object"]) != obj.id or track["property"] not in {"position", "reveal"}:
            continue
        # Cleanup holds from an earlier passing flash intentionally remain active.
        # Later authored tracks supersede them by ID while preserving the restored
        # semantic state before the next moving window begins.
        if ".cleanup." in _track_key(scene, track):
            continue
        if _overlaps(track["timing"], start, end):
            raise ValueError(
                "ShowPassingFlash cannot overlap Position/Reveal animation on the same Line"
            )


def _time_map_step(start: float, duration: float, rate_id: str) -> dict[str, Any]:
    return {
        "start": float(start),
        "duration": float(duration),
        "rate_func": str(rate_id),
    }


def _attach_piece_map(
    scene: _compat.Scene,
    track_index: int,
    *,
    prefix: list[dict[str, Any]],
    local_start: float,
    local_duration: float,
    rate_id: str,
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] | None,
) -> None:
    steps = copy.deepcopy(prefix)
    steps.append(_time_map_step(local_start, local_duration, rate_id))
    if pending_time_maps is None:
        scene._tracks[track_index]["time_map"] = {"steps": steps}
    else:
        pending_time_maps.append((track_index, track_index + 1, steps))


def _add_mapped_scalar(
    scene: _compat.Scene,
    obj: _base._ir.Object,
    property_name: str,
    from_: float,
    to: float,
    *,
    root_start: float,
    root_duration: float,
    local_start: float,
    local_duration: float,
    rate_id: str,
    key: str,
    prefix: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] | None,
) -> None:
    index = len(scene._tracks)
    scene._add_scalar_track(
        obj,
        property_name,
        float(from_),
        float(to),
        root_start,
        root_duration,
        "linear",
        key,
    )
    _attach_piece_map(
        scene,
        index,
        prefix=prefix,
        local_start=local_start,
        local_duration=local_duration,
        rate_id=rate_id,
        pending_time_maps=pending_time_maps,
    )


def _add_mapped_position(
    scene: _compat.Scene,
    obj: _base._ir.Object,
    from_: tuple[float, float],
    to: tuple[float, float],
    *,
    root_start: float,
    root_duration: float,
    local_start: float,
    local_duration: float,
    rate_id: str,
    key: str,
    prefix: list[dict[str, Any]],
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] | None,
) -> None:
    index = len(scene._tracks)
    scene._add_track(
        obj,
        "position",
        {
            "vec2": {
                "from": {"x": float(from_[0]), "y": float(from_[1])},
                "to": {"x": float(to[0]), "y": float(to[1])},
            }
        },
        root_start,
        root_duration,
        "linear",
        key,
    )
    _attach_piece_map(
        scene,
        index,
        prefix=prefix,
        local_start=local_start,
        local_duration=local_duration,
        rate_id=rate_id,
        pending_time_maps=pending_time_maps,
    )


def _transformed_line_delta(snapshot: dict[str, Any]) -> tuple[float, float]:
    line = snapshot["geometry"]["line"]
    start = line["start"]
    end = line["end"]
    delta_x = float(end["x"]) - float(start["x"])
    delta_y = float(end["y"]) - float(start["y"])
    transform = snapshot["transform"]
    scale = transform["scale"]
    scaled_x = delta_x * float(scale["x"])
    scaled_y = delta_y * float(scale["y"])
    rotation = float(transform["rotation"])
    cosine = math.cos(rotation)
    sine = math.sin(rotation)
    return (
        cosine * scaled_x - sine * scaled_y,
        sine * scaled_x + cosine * scaled_y,
    )


def _schedule_show_passing_flash(
    scene: _compat.Scene,
    animation: ShowPassingFlash,
    *,
    start_time: float,
    run_time: float,
    rate_id: str,
    time_map_prefix: list[dict[str, Any]] | None = None,
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] | None = None,
) -> None:
    start = float(start_time)
    duration = float(run_time)
    if not math.isfinite(start) or start < 0.0:
        raise ValueError("start_time must be finite and non-negative")
    if not math.isfinite(duration) or duration <= 0.0:
        raise ValueError("ShowPassingFlash run_time must be finite and positive")

    prefix = [] if time_map_prefix is None else list(time_map_prefix)
    if pending_time_maps is not None and any(
        step.get("rate_func") != "linear" for step in prefix
    ):
        raise NotImplementedError(
            "ShowPassingFlash inside a nonlinear outer composition remains partial "
            "until lifecycle cleanup events carry the same time map"
        )

    member = animation.mobject
    has_updaters = getattr(member, "has_updaters", None)
    if callable(has_updaters) and has_updaters():
        raise NotImplementedError(
            "ShowPassingFlash with concurrent user updaters remains partial"
        )

    # Match ordinary Scene.play implicit addition plus ShowPassingFlash's
    # introducer/remover behavior. The shared lifecycle planner remains the source
    # of truth for reintroduction and exact-end removal.
    _animate._bind_for_animation(scene, member, start_time=start)
    assert member._object is not None
    obj = member._object
    removal = _lifecycle._resolve_wrapper(
        scene,
        member,
        "remove_after_animation",
        start,
        "ShowPassingFlash target",
    )

    end = start + duration
    _ensure_window_channels_available(scene, obj, start=start, end=end)
    snapshot = scene._snapshot_for_object_at(obj, start)
    if "line" not in snapshot["geometry"]:
        raise NotImplementedError("ShowPassingFlash currently requires retained Line geometry")

    translation = snapshot["transform"]["translation"]
    original_position = (float(translation["x"]), float(translation["y"]))
    delta = _transformed_line_delta(snapshot)
    shifted_position = (
        original_position[0] + delta[0],
        original_position[1] + delta[1],
    )

    tw = animation.time_width
    denominator = 1.0 + tw
    peak = min(tw, 1.0)
    first_end = peak / denominator
    last_start = max(tw, 1.0) / denominator
    lower_start = tw / denominator
    object_key = scene._object_keys[obj.id]
    root_key = f"@show-passing-flash:{object_key}:{start:g}"

    # Window width: ramp 0 -> min(time_width, 1), hold while both clamps are
    # inactive/active, then ramp back to zero. The hold needs no track: the first
    # mapped segment remains at its `to` value until the later segment has begun.
    _add_mapped_scalar(
        scene,
        obj,
        "reveal",
        0.0,
        peak,
        root_start=start,
        root_duration=duration,
        local_start=0.0,
        local_duration=first_end,
        rate_id=rate_id,
        key=f"{root_key}.width-in",
        prefix=prefix,
        pending_time_maps=pending_time_maps,
    )
    _add_mapped_scalar(
        scene,
        obj,
        "reveal",
        peak,
        0.0,
        root_start=start,
        root_duration=duration,
        local_start=last_start,
        local_duration=1.0 - last_start,
        rate_id=rate_id,
        key=f"{root_key}.width-out",
        prefix=prefix,
        pending_time_maps=pending_time_maps,
    )

    # The lower bound starts moving at time_width / (1 + time_width) in *warped*
    # animation alpha. Translating by one transformed line direction as local
    # progress goes 0 -> 1 is therefore exactly lower(alpha).
    _add_mapped_position(
        scene,
        obj,
        original_position,
        shifted_position,
        root_start=start,
        root_duration=duration,
        local_start=lower_start,
        local_duration=1.0 - lower_start,
        rate_id=rate_id,
        key=f"{root_key}.lower",
        prefix=prefix,
        pending_time_maps=pending_time_maps,
    )

    # FadeOut leaves Noon's renderer appearance at zero after removal. Manim's
    # ShowPartial operates on the restored mobject style, so reintroduction must not
    # inherit that renderer-only state.
    if scene._appearance_at(obj, start) != 1.0:
        scene._add_scalar_track(
            obj,
            "appearance",
            1.0,
            1.0,
            start,
            duration,
            "linear",
            f"{root_key}.appearance",
        )

    # Manim finishes the zero-width partial state, restores the full starting
    # VMobject in clean_up_from_scene, then removes it. Position/Reveal are
    # continuous properties in Noon, so use constant hidden cleanup holds beginning
    # at the exact remover boundary. They do not advance Scene.time and later authored
    # tracks at the same boundary supersede them deterministically by track ID.
    scene._add_scalar_track(
        obj,
        "reveal",
        1.0,
        1.0,
        end,
        duration,
        "linear",
        f"{root_key}.cleanup.reveal",
    )
    scene._add_track(
        obj,
        "position",
        {
            "vec2": {
                "from": {"x": original_position[0], "y": original_position[1]},
                "to": {"x": original_position[0], "y": original_position[1]},
            }
        },
        end,
        duration,
        "linear",
        f"{root_key}.cleanup.position",
    )

    if removal.hide_at_end:
        scene._add_presence_track(
            obj,
            True,
            False,
            end,
            key=f"{root_key}.hide",
        )

    scene._compat_top_level = [
        value for value in scene._compat_top_level if value is not member
    ]


def _resolve_options(
    animation: ShowPassingFlash,
    *,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
):
    resolved = _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=0.0,
        play_run_time=play_run_time,
        play_easing=play_easing,
        play_rate_func=play_rate_func,
        play_lag_ratio=play_lag_ratio,
    )
    if resolved.reverse_rate_function:
        raise NotImplementedError(
            "ShowPassingFlash reverse_rate_function=True remains partial"
        )
    return resolved


def _show_passing_flash_scene_play(
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
    flashes = [animation for animation in animations if isinstance(animation, ShowPassingFlash)]
    if not flashes:
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
    if len(flashes) != len(animations):
        raise NotImplementedError(
            "mixing top-level ShowPassingFlash with unrelated animations remains partial; "
            "use AnimationGroup"
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
        play_lag_ratio = float(lag_ratio)
    else:
        play_lag_ratio = None
    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")

    checkpoint = self._authoring_checkpoint()
    cursor_before = self._cursor
    top_level_before = list(self._compat_top_level)
    wrapper_states: dict[int, tuple[_base.Mobject, object, object]] = {}
    for animation in flashes:
        _animate._record_wrapper_state(animation.mobject, wrapper_states)

    max_end = base_start
    try:
        for animation in flashes:
            resolved = _resolve_options(
                animation,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=play_lag_ratio,
            )
            _schedule_show_passing_flash(
                self,
                animation,
                start_time=base_start,
                run_time=resolved.run_time,
                rate_id=resolved.rate_func,
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
    if not isinstance(animation, ShowPassingFlash):
        _ORIGINAL_COMPOSITION_PLAY_LEAF(
            scene,
            animation,
            start_time=start_time,
            run_time=run_time,
            time_map_steps=time_map_steps,
            pending_time_maps=pending_time_maps,
        )
        return

    resolved = _resolve_options(
        animation,
        play_run_time=run_time,
        play_easing=None,
        play_rate_func=None,
        play_lag_ratio=None,
    )
    _schedule_show_passing_flash(
        scene,
        animation,
        start_time=start_time,
        run_time=run_time,
        rate_id=resolved.rate_func,
        time_map_prefix=time_map_steps,
        pending_time_maps=pending_time_maps,
    )


def _record_composition_wrapper_state(
    animation: object,
    states: dict[int, tuple[_base.Mobject, object, object]],
) -> None:
    if isinstance(animation, ShowPassingFlash):
        _animate._record_wrapper_state(animation.mobject, states)
        return
    _ORIGINAL_RECORD_COMPOSITION_WRAPPER_STATE(animation, states)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    public = {
        "Flash": Flash,
        "ShowPassingFlash": ShowPassingFlash,
    }
    for name, value in public.items():
        setattr(_base, name, value)
        setattr(_compat, name, value)
        if name not in _base.__all__:
            _base.__all__.append(name)
    _compat.Scene.play = _show_passing_flash_scene_play
    _composition._play_leaf = _composition_play_leaf
    _composition._record_composition_wrapper_state = _record_composition_wrapper_state
    _INSTALLED = True
