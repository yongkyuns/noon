"""ManimCE v0.21 thinning passing-flash compatibility for retained Lines.

The upstream helper is an ``AnimationGroup`` of copied VMobjects whose stroke widths
increase while their ``ShowPassingFlash.time_width`` values decrease to exactly zero.
Noon keeps this slice deterministic: positive-width children use the retained Line
window implementation, while the zero-width endpoint is represented by a fully hidden
reveal track followed by ordinary ShowPassingFlash cleanup/removal.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_composition as _composition
import _manim_indication as _indication
import _manim_lifecycle as _lifecycle
import _manim_phase_b as _phase_b


_INSTALLED = False
_ORIGINAL_SHOW_PASSING_FLASH_INIT = _indication.ShowPassingFlash.__init__
_ORIGINAL_SCHEDULE_SHOW_PASSING_FLASH = _indication._schedule_show_passing_flash


def _show_passing_flash_init(
    self: _indication.ShowPassingFlash,
    mobject: object,
    time_width: float = 0.1,
    **kwargs: Any,
) -> None:
    width = float(time_width)
    if width != 0.0:
        _ORIGINAL_SHOW_PASSING_FLASH_INIT(
            self,
            mobject,
            time_width=width,
            **kwargs,
        )
        return

    # The base retained slice already owns all positive widths. Zero is a meaningful
    # Manim edge case (lower == upper for every alpha), needed by the thinning helper.
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
    if bool(kwargs.get("reverse_rate_function", False)):
        raise NotImplementedError(
            "ShowPassingFlash reverse_rate_function=True remains partial"
        )
    if "path_arc" in kwargs:
        raise TypeError("ShowPassingFlash does not accept path_arc")

    self.mobject = mobject
    self.target = mobject
    self.time_width = 0.0
    self.remover = True
    self.introducer = True
    self.anim_args = dict(kwargs)


def _schedule_show_passing_flash(
    scene: _compat.Scene,
    animation: _indication.ShowPassingFlash,
    *,
    start_time: float,
    run_time: float,
    rate_id: str,
    time_map_prefix: list[dict[str, Any]] | None = None,
    pending_time_maps: list[tuple[int, int, list[dict[str, Any]]]] | None = None,
) -> None:
    if animation.time_width != 0.0:
        _ORIGINAL_SCHEDULE_SHOW_PASSING_FLASH(
            scene,
            animation,
            start_time=start_time,
            run_time=run_time,
            rate_id=rate_id,
            time_map_prefix=time_map_prefix,
            pending_time_maps=pending_time_maps,
        )
        return

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
    _indication._ensure_window_channels_available(scene, obj, start=start, end=end)
    snapshot = scene._snapshot_for_object_at(obj, start)
    if "line" not in snapshot["geometry"]:
        raise NotImplementedError("ShowPassingFlash currently requires retained Line geometry")

    object_key = scene._object_keys[obj.id]
    root_key = f"@show-passing-flash:{object_key}:{start:g}"

    # For tw=0, Manim's bounds are (alpha, alpha) for every frame: the partial path
    # has zero length throughout. A constant zero reveal is therefore exact and is
    # independent of the child rate function.
    scene._add_scalar_track(
        obj,
        "reveal",
        0.0,
        0.0,
        start,
        duration,
        "linear",
        f"{root_key}.zero-width",
    )

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

    # clean_up_from_scene restores the full source VMobject before remover cleanup.
    # Geometry/position never moved in the zero-width lowering, so only Reveal needs
    # an explicit restored hold.
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


def _linspace(start: float, stop: float, count: int) -> list[float]:
    if count == 1:
        return [float(start)]
    return [
        float(start) + (float(stop) - float(start)) * index / (count - 1)
        for index in range(count)
    ]


def _authored_stroke_width(vmobject: _compat.VMobject) -> float:
    raw_width = float(vmobject._current_raw().style["stroke_width"])
    return raw_width / _phase_b.MANIM_CAIRO_LINE_WIDTH_MULTIPLE


class ShowPassingFlashWithThinningStrokeWidth(_composition.AnimationGroup):
    """Layer retained Line flashes with the Manim thinning-stroke construction."""

    def __init__(
        self,
        vmobject: object,
        n_segments: int = 10,
        time_width: float = 0.1,
        remover: bool = True,
        **kwargs: Any,
    ) -> None:
        if isinstance(vmobject, _compat.Group):
            raise NotImplementedError(
                "ShowPassingFlashWithThinningStrokeWidth retained family semantics remain partial"
            )
        if not isinstance(vmobject, _compat.VMobject):
            raise TypeError(
                "ShowPassingFlashWithThinningStrokeWidth requires a VMobject"
            )
        if "line" not in vmobject._current_raw().geometry:
            raise NotImplementedError(
                "ShowPassingFlashWithThinningStrokeWidth currently qualifies the exact Line subset"
            )
        if isinstance(n_segments, bool) or not isinstance(n_segments, int):
            raise TypeError("n_segments must be an integer")
        if n_segments <= 0:
            raise ValueError("n_segments must be positive")

        width = float(time_width)
        if not math.isfinite(width) or width < 0.0:
            raise ValueError("time_width must be finite and non-negative")

        self.n_segments = n_segments
        self.time_width = width
        self.remover = bool(remover)

        max_stroke_width = _authored_stroke_width(vmobject)
        animation_kwargs = dict(kwargs)
        max_time_width = float(animation_kwargs.pop("time_width", self.time_width))
        if not math.isfinite(max_time_width) or max_time_width < 0.0:
            raise ValueError("time_width must be finite and non-negative")

        stroke_widths = _linspace(0.0, max_stroke_width, self.n_segments)
        time_widths = _linspace(max_time_width, 0.0, self.n_segments)
        animations = [
            _indication.ShowPassingFlash(
                vmobject.copy().set_stroke(width=stroke_width),
                time_width=segment_time_width,
                **animation_kwargs,
            )
            for stroke_width, segment_time_width in zip(
                stroke_widths,
                time_widths,
                strict=True,
            )
        ]
        super().__init__(*animations)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return

    _indication.ShowPassingFlash.__init__ = _show_passing_flash_init
    _indication._schedule_show_passing_flash = _schedule_show_passing_flash

    name = "ShowPassingFlashWithThinningStrokeWidth"
    setattr(_base, name, ShowPassingFlashWithThinningStrokeWidth)
    setattr(_compat, name, ShowPassingFlashWithThinningStrokeWidth)
    if name not in _base.__all__:
        _base.__all__.append(name)

    _INSTALLED = True
