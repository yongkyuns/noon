"""Thin Python adapter for Noon's shared Rust animation-option resolver."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from js import noonResolveAnimationOptions as _resolve_shared_animation_options

import _manim_compat as _compat


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
    return _compat._easing_from_rate_func(args["rate_func"])


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
        play_rate_id = _compat._easing_from_rate_func(play_rate_func)

    result = _resolve_shared_animation_options(
        float(default_lag_ratio),
        _optional_number(builder_args, "run_time"),
        _optional_rate_func(builder_args),
        _optional_number(builder_args, "lag_ratio"),
        _optional_number(builder_args, "path_arc"),
        _optional_reverse(builder_args),
        float("nan") if play_run_time is None else float(play_run_time),
        play_rate_id,
        float("nan") if play_lag_ratio is None else float(play_lag_ratio),
    )

    if not bool(result.ok):
        message = str(result.message)
        if str(result.errorKind) == "unsupported":
            raise NotImplementedError(message)
        raise ValueError(message)

    return ResolvedAnimationOptions(
        run_time=float(result.runTime),
        rate_func=str(result.rateFunc),
        lag_ratio=float(result.lagRatio),
        path_arc=float(result.pathArc),
        reverse_rate_function=bool(result.reverseRateFunction),
    )
