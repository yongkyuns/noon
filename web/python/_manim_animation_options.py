"""Thin Python adapter for Noon's shared Rust animation-option resolver."""

from __future__ import annotations

from dataclasses import dataclass
import math
from typing import Any

from js import noonResolveAnimationOptions as _resolve_shared_animation_options

import noon as _base
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


def _scale_in_place_builder(
    mobject: object,
    scale_factor: float,
    animation_kwargs: dict[str, Any],
) -> object:
    """Lower Manim's ApplyMethod-based scale helpers to the target-state builder.

    ManimCE v0.21 implements ``ScaleInPlace`` as ``ApplyMethod(mobject.scale, factor)``
    and ``ShrinkToCenter`` as ``ScaleInPlace(..., 0)``. For a single 2D leaf whose
    authored geometry is centered on its transform origin, Noon's ordinary target-state
    builder represents the same endpoint and interpolation without a new playback path.
    """

    if isinstance(mobject, _compat.Group):
        raise NotImplementedError(
            "ScaleInPlace/ShrinkToCenter Group/VGroup family scaling is not yet supported"
        )
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("ScaleInPlace/ShrinkToCenter target must be a Mobject")

    factor = float(scale_factor)
    if not math.isfinite(factor):
        raise ValueError("scale factor must be finite")

    # Import lazily to avoid a cycle: this adapter is imported by _manim_animate.
    # Calls happen only after the animation module has finished installing its aligned
    # builder, so we reuse the exact same implicit-binding, rollback, option-resolution,
    # and retained Transform lowering as ``mobject.animate.scale(...)``.
    import _manim_animate as _animate

    builder = _animate._AlignedAnimationBuilder(mobject)
    builder.anim_args = dict(animation_kwargs)
    builder.cannot_pass_args = True
    builder.target.scale(factor)
    return builder


def ScaleInPlace(mobject: object, scale_factor: float, **kwargs: Any) -> object:
    """Scale one detached or retained 2D leaf in place using Manim timing options."""

    return _scale_in_place_builder(mobject, scale_factor, kwargs)


def ShrinkToCenter(mobject: object, **kwargs: Any) -> object:
    """Shrink one 2D leaf to its center, matching Manim's zero-scale helper."""

    return _scale_in_place_builder(mobject, 0.0, kwargs)


def FadeToColor(mobject: object, color: object, **kwargs: Any) -> object:
    """Animate one 2D leaf to ``color`` through the retained target-state transform.

    ManimCE v0.21 defines ``FadeToColor`` as
    ``ApplyMethod(mobject.set_color, color, **kwargs)``.  For a retained leaf, Noon's
    aligned animation builder captures the same target style while keeping timing,
    implicit scene binding, rollback, and deterministic seek behavior on the ordinary
    Transform path.
    """

    if isinstance(mobject, _compat.Group):
        raise NotImplementedError(
            "FadeToColor Group/VGroup family recoloring is not yet supported"
        )
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("FadeToColor target must be a Mobject")

    import _manim_animate as _animate

    builder = _animate._AlignedAnimationBuilder(mobject)
    builder.anim_args = dict(kwargs)
    builder.cannot_pass_args = True
    builder.target.set_color(color)
    return builder


def Restore(mobject: object, **kwargs: Any) -> object:
    """Transform one 2D leaf back to its most recently saved Manim state.

    ManimCE v0.21 defines ``Restore`` as ``ApplyMethod(mobject.restore, **kwargs)``.
    The compatibility layer already owns ``save_state``/``restore`` and stores the
    detached semantic clone on the mobject, so the ordinary target-state builder can
    snapshot the same restore target without runtime host callbacks.
    """

    if isinstance(mobject, _compat.Group):
        raise NotImplementedError("Restore Group/VGroup family state is not yet supported")
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("Restore target must be a Mobject")

    import _manim_animate as _animate

    builder = _animate._AlignedAnimationBuilder(mobject)
    builder.anim_args = dict(kwargs)
    builder.cannot_pass_args = True
    # Let the compatibility method preserve Manim's missing-save error semantics.
    builder.target.restore()
    return builder


for _name, _value in {
    "ScaleInPlace": ScaleInPlace,
    "ShrinkToCenter": ShrinkToCenter,
    "FadeToColor": FadeToColor,
    "Restore": Restore,
}.items():
    setattr(_base, _name, _value)
    setattr(_compat, _name, _value)
    if _name not in _base.__all__:
        _base.__all__.append(_name)
