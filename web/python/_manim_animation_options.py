"""Thin Python adapter for Noon's shared Rust animation-option resolver."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from js import noonResolveAnimationOptions as _resolve_shared_animation_options

try:
    from js import noonResolveTransformAnimationOptions as _resolve_transform_animation_options
except ImportError:  # Test doubles predating the Transform-only bridge.
    _resolve_transform_animation_options = None

import _manim_rate_functions as _rate_functions
from _noon_errors import engine_call


_SUPPORTED_BUILDER_ARGS = {
    "run_time",
    "rate_func",
    "lag_ratio",
    "path_arc",
    "reverse_rate_function",
    # These are accepted Python/Manim metadata today. They do not change Noon's
    # deterministic timing/lowering yet, so they intentionally stay adapter-only.
    "suspend_mobject_updating",
    "name",
}


@dataclass(frozen=True)
class ResolvedAnimationOptions:
    run_time: float
    rate_func: str
    lag_ratio: float
    path_arc: float
    reverse_rate_function: bool


def builder_args(builder: object) -> dict[str, Any]:
    args = dict(getattr(builder, "anim_args", {}))
    unsupported = sorted(set(args) - _SUPPORTED_BUILDER_ARGS)
    if unsupported:
        raise NotImplementedError(
            "unsupported Manim .animate option(s): " + ", ".join(unsupported)
        )
    return args


def _optional_number(args: dict[str, Any], name: str) -> float:
    if name not in args:
        return float("nan")
    return float(args[name])


def _optional_rate_func(args: dict[str, Any]) -> str:
    if "rate_func" not in args:
        return ""
    return _rate_functions.easing_from_rate_func(args["rate_func"])


def _optional_reverse(args: dict[str, Any]) -> int:
    if "reverse_rate_function" not in args:
        return -1
    return 1 if bool(args["reverse_rate_function"]) else 0


def resolve(
    *,
    builder_args: dict[str, Any],
    default_lag_ratio: float,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
) -> ResolvedAnimationOptions:
    play_rate_id = ""
    if play_easing is not None:
        play_rate_id = str(play_easing)
    elif play_rate_func is not None:
        play_rate_id = _rate_functions.easing_from_rate_func(play_rate_func)

    result = engine_call(
        _resolve_shared_animation_options,
        float(default_lag_ratio),
        _optional_number(builder_args, "run_time"),
        _optional_rate_func(builder_args),
        _optional_number(builder_args, "lag_ratio"),
        _optional_number(builder_args, "path_arc"),
        _optional_reverse(builder_args),
        float("nan") if play_run_time is None else float(play_run_time),
        play_rate_id,
        float("nan") if play_lag_ratio is None else float(play_lag_ratio),
        operation="animation.options",
    )

    return ResolvedAnimationOptions(
        run_time=float(result.runTime),
        rate_func=str(result.rateFunc),
        lag_ratio=float(result.lagRatio),
        path_arc=float(result.pathArc),
        reverse_rate_function=bool(result.reverseRateFunction),
    )


def resolve_transform(
    *,
    builder_args: dict[str, Any],
    default_lag_ratio: float,
    play_run_time: float | None,
    play_easing: str | None,
    play_rate_func: object | None,
    play_lag_ratio: float | None,
    play_path_arc: float | None,
) -> ResolvedAnimationOptions:
    if _resolve_transform_animation_options is None:
        animation_path_arc = float(builder_args.get("path_arc", 0.0))
        effective_path_arc = (
            float(play_path_arc) if play_path_arc is not None else animation_path_arc
        )
        if effective_path_arc != 0.0:
            raise NotImplementedError("Transform path-arc option bridge is unavailable")
        return resolve(
            builder_args=builder_args,
            default_lag_ratio=default_lag_ratio,
            play_run_time=play_run_time,
            play_easing=play_easing,
            play_rate_func=play_rate_func,
            play_lag_ratio=play_lag_ratio,
        )
    play_rate_id = ""
    if play_easing is not None:
        play_rate_id = str(play_easing)
    elif play_rate_func is not None:
        play_rate_id = _rate_functions.easing_from_rate_func(play_rate_func)

    result = engine_call(
        _resolve_transform_animation_options,
        float(default_lag_ratio),
        _optional_number(builder_args, "run_time"),
        _optional_rate_func(builder_args),
        _optional_number(builder_args, "lag_ratio"),
        _optional_number(builder_args, "path_arc"),
        _optional_reverse(builder_args),
        float("nan") if play_run_time is None else float(play_run_time),
        play_rate_id,
        float("nan") if play_lag_ratio is None else float(play_lag_ratio),
        float("nan") if play_path_arc is None else float(play_path_arc),
        operation="animation.transform_options",
    )
    return ResolvedAnimationOptions(
        run_time=float(result.runTime),
        rate_func=str(result.rateFunc),
        lag_ratio=float(result.lagRatio),
        path_arc=float(result.pathArc),
        reverse_rate_function=bool(result.reverseRateFunction),
    )
