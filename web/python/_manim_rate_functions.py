from __future__ import annotations

import copy
import math
from collections.abc import Callable
from typing import Any

import _manim_compat as _compat
import _noon_ir as _ir
import noon as _base


_INSTALLED = False


def linear(t: float) -> float:
    return float(t)


def smooth(t: float) -> float:
    t = float(t)
    if t <= 0.0:
        return 0.0
    if t >= 1.0:
        return 1.0
    error = 1.0e-4
    sigmoid = lambda x: 1.0 / (1.0 + math.exp(-x))
    return float((sigmoid(-6.0 * t + 3.0) - error) / (1.0 - 2.0 * error))


def rush_into(t: float) -> float:
    return float(2.0 * smooth(float(t) / 2.0))


def rush_from(t: float) -> float:
    return float(2.0 * smooth(float(t) / 2.0 + 0.5) - 1.0)


def there_and_back(t: float) -> float:
    t = float(t)
    new_t = 2.0 * t if t < 0.5 else 2.0 * (1.0 - t)
    return float(smooth(new_t))


def easing_from_rate_func(rate_func: Callable[[float], float] | None) -> str:
    if rate_func is None or rate_func is smooth:
        return "smooth"
    if rate_func is linear:
        return "linear"
    if rate_func is rush_into:
        return "rush_into"
    if rate_func is rush_from:
        return "rush_from"
    if rate_func is there_and_back:
        return "there_and_back"
    raise NotImplementedError(
        "custom Python rate functions are not supported by the native retained runtime; "
        "use a supported Noon/Manim rate function"
    )


def _track_progress(
    easing: str,
    value: Any,
) -> float:
    t = float(value)
    if easing == "linear":
        return t
    if easing == "smooth":
        return smooth(t)
    if easing == "rush_into":
        return rush_into(t)
    if easing == "rush_from":
        return rush_from(t)
    if easing == "there_and_back":
        return there_and_back(t)
    return float(_ir._ORIGINAL_TRACK_PROGRESS(easing, t))


def _add_track(
    self: _ir.Scene,
    object_id: int,
    prop: _ir.Property,
    start_time: float,
    duration: float,
    easing: str,
    from_value: Any,
    to_value: Any,
    *,
    replace: bool,
    presence: str | None,
    time_map: dict[str, Any] | None = None,
) -> None:
    from_value = copy.deepcopy(from_value)
    to_value = copy.deepcopy(to_value)
    if replace:
        self._tracks = [
            track
            for track in self._tracks
            if not (track["object"] == object_id and track["property"] == prop)
        ]
    track = {
        "object": object_id,
        "property": prop,
        "timing": {
            "start": float(start_time),
            "duration": float(duration),
            "easing": easing,
        },
        "values": {
            "from": from_value,
            "to": to_value,
        },
    }
    if presence is not None:
        track["presence"] = presence
    if time_map is not None:
        track["time_map"] = copy.deepcopy(time_map)
    self._tracks.append(track)


def _set_color_preserving_opacity(self: _base.Mobject, color: str) -> _base.Mobject:
    result = self
    raw = copy.deepcopy(result._raw)
    style = raw.setdefault("style", {})
    fill = style.get("fill")
    stroke = style.get("stroke")
    previous_fill_alpha = fill.get("alpha") if isinstance(fill, dict) else None
    previous_stroke_alpha = stroke.get("alpha") if isinstance(stroke, dict) else None
    result._apply(raw)
    result.set_fill(color=color)
    result.set_stroke(color=color)
    raw = copy.deepcopy(result._raw)
    style = raw.setdefault("style", {})
    if previous_fill_alpha is not None and isinstance(style.get("fill"), dict):
        style["fill"]["alpha"] = previous_fill_alpha
    if previous_stroke_alpha is not None and isinstance(style.get("stroke"), dict):
        style["stroke"]["alpha"] = previous_stroke_alpha
    result._apply(raw)
    return result


class MoveToTarget:
    """ManimCE ``MoveToTarget`` as a thin adapter over retained ``Transform``."""

    def __new__(cls, mobject: object, **kwargs: Any):
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError(
                "MoveToTarget(Group/VGroup) requires retained family Transform semantics and is not yet supported"
            )
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("MoveToTarget target must be a Mobject")
        if not hasattr(mobject, "target"):
            raise ValueError("MoveToTarget called on mobjectwithout attribute 'target'")
        target = mobject.target
        if not isinstance(target, _base.Mobject) or isinstance(target, _compat.Group):
            raise NotImplementedError(
                "MoveToTarget currently requires a leaf Mobject target produced by generate_target()"
            )
        return _base.Transform(mobject, target, **kwargs)


def install() -> None:
    """Install thin public adapters without creating a second playback engine."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    public = {
        "linear": linear,
        "smooth": smooth,
        "rush_into": rush_into,
        "rush_from": rush_from,
        "there_and_back": there_and_back,
        "MoveToTarget": MoveToTarget,
    }
    for name, value in public.items():
        setattr(_compat, name, value)
        setattr(_base, name, value)

    _compat._easing_from_rate_func = easing_from_rate_func

    # Manim's set_color changes fill/stroke RGB while preserving each channel's
    # independent opacity. This matters for Transform-style effects such as Indicate:
    # a transparent stroke must not become visible while the color interpolates.
    _base.Mobject.set_color = _set_color_preserving_opacity

    # `_noon_ir` needs progress only while materializing authoring-time snapshots.
    # Runtime playback never calls this mirror; Rust RateFunction remains authoritative.
    _ir._track_progress = _track_progress
    _ir.Scene._add_track = _add_track

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports