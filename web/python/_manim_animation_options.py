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
    """Lower Manim's ApplyMethod-based scale helpers to a deferred target builder.

    ManimCE v0.21 implements ``ScaleInPlace`` as ``ApplyMethod(mobject.scale, factor)``.
    ``ApplyMethod.create_target``
    runs from ``Transform.begin()``, so the target must be copied from the mobject state
    that exists when ``Scene.play`` begins rather than when the animation is constructed.
    For a single 2D leaf, Noon's retained scene snapshot plus the ordinary target-state
    Transform path represents the same endpoint/interpolation without a new playback path.
    """

    if not isinstance(mobject, (_base.Mobject, _compat.Group)):
        raise TypeError("ScaleInPlace target must be a Mobject or Group")

    factor = float(scale_factor)
    if not math.isfinite(factor):
        raise ValueError("scale factor must be finite")

    # Retained Text/Typst uses a source-level sidecar rather than legacy geometry.
    # Install its scheduler lazily at the first retained scale helper so ordinary
    # geometry-only authoring and worker startup remain untouched.
    import _manim_typst as _typst

    if isinstance(mobject, _typst._RetainedTextMobject):
        import _manim_retained_animate as _retained_animate

        _retained_animate.install()

    # Import lazily to avoid a cycle: this adapter is imported by _manim_animate.
    # Calls happen only after the animation module has finished installing its aligned
    # builder. Subclassing that builder keeps implicit binding, rollback, shared option
    # resolution, and retained Transform lowering unchanged while deferring only target
    # materialization to the point where the scheduler asks for ``animation.target``.
    import _manim_animate as _animate

    builder_base = (
        _animate._AlignedGroupAnimationBuilder
        if isinstance(mobject, _compat.Group)
        else _animate._AlignedAnimationBuilder
    )

    class _DeferredScaleBuilder(builder_base):
        def __init__(self) -> None:
            # Do not call _AlignedAnimationBuilder.__init__: its normal .animate path
            # eagerly copies the source because chained methods are authored immediately.
            # ApplyMethod-family wrappers instead copy at Transform.begin/play time.
            self.source = mobject
            self.mobject = mobject
            self.scale_factor = factor
            self.anim_args = dict(animation_kwargs)
            self.cannot_pass_args = True
            self.is_chaining = False

        @property
        def target(self) -> object:
            source = self.source
            if isinstance(source, _compat.Group):
                target = source._copy_for_animate_target()
                target.scale(self.scale_factor)
                return target
            if (
                getattr(source, "_semantic_handle", None) is not None
                and getattr(source, "_semantic_handle_fresh", False)
                and getattr(getattr(source, "_scene", None), "_canonical_authoring_context", None)
                is not None
            ):
                target = source._copy_for_animate_target()
                target.scale(self.scale_factor)
                return target
            scene = source._scene
            obj = source._object
            if scene is not None and obj is not None:
                # _aligned_scene_play binds method-animation sources before expanding
                # them. The cursor is therefore the Manim-compatible play-begin time and
                # the retained snapshot includes all earlier authored animations.
                snapshot = scene._snapshot_for_object_at(obj, scene._cursor)
                target = _animate._snapshot_mobject(snapshot)
            else:
                # This fallback is mainly useful for introspection outside Scene.play;
                # normal scheduling reaches the bound retained-snapshot branch above.
                target = source.copy()
            target.scale(self.scale_factor)
            return target

    return _DeferredScaleBuilder()


def ScaleInPlace(mobject: object, scale_factor: float, **kwargs: Any) -> object:
    """Scale one detached or retained 2D leaf in place using Manim timing options."""

    return _scale_in_place_builder(mobject, scale_factor, kwargs)


class ShrinkToCenter:
    """Inert request for the shared Rust scale-to-center removal lifecycle."""

    _canonical_affine_lifecycle = "shrink"

    def __init__(self, mobject: object, **kwargs: Any) -> None:
        if isinstance(mobject, _compat.Group):
            raise NotImplementedError("ShrinkToCenter currently supports one leaf Mobject")
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("ShrinkToCenter target must be a Mobject")
        self.mobject = mobject
        self.anim_args = dict(kwargs)


public = {
    "ScaleInPlace": ScaleInPlace,
    "ShrinkToCenter": ShrinkToCenter,
}
for _name, _value in public.items():
    setattr(_base, _name, _value)
    setattr(_compat, _name, _value)
    if _name not in _base.__all__:
        _base.__all__.append(_name)
