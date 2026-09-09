"""Manim animation requests and chained `.animate` call-shape adaptation.

Python carries inert options and target handles. Shared Rust semantics owns
validation, activation, scheduling, lifecycle, and deterministic playback.
"""

from __future__ import annotations

import math
from typing import Any, Callable

import noon as _base
import _manim_compat as _compat
import _manim_rate_functions as _rate_functions


_ORIGINAL_TRANSFORM = _base.Transform
_ORIGINAL_REPLACEMENT_TRANSFORM = _base.ReplacementTransform
_ORIGINAL_TRANSFORM_FROM_COPY = _base.TransformFromCopy
_ORIGINAL_TRANSFORM_MATCHING_SHAPES = _base.TransformMatchingShapes
_ORIGINAL_CREATE = _base.Create
_ORIGINAL_UNCREATE = _base.Uncreate
_ORIGINAL_FADE_IN = _base.FadeIn
_ORIGINAL_FADE_OUT = _base.FadeOut
_PURE_YELLOW = _base.color_from_hex("#FFFF00")


def _store_animation_args(animation: object, kwargs: dict[str, Any]) -> None:
    """Attach Manim Animation kwargs for the shared option resolver.

    Noon's original animation records are frozen/slotted dataclasses. Subclassing them
    gives the compatibility facade a small Python ``__dict__`` for authoring metadata
    while preserving the existing low-level fields and isinstance-based lowering.
    Validation remains centralized in ``_manim_animation_options`` at play time.
    """

    object.__setattr__(animation, "anim_args", dict(kwargs))


def _fade_authoring_options(
    target: object, kwargs: dict[str, Any]
) -> tuple[dict[str, Any], _base.Vec2, float, bool, _base.Vec2 | None]:
    """Separate `_Fade` endpoint options from generic Animation options.

    Manim resolves a Mobject ``target_position`` to a point during `_Fade.__init__`,
    while an explicitly supplied ``shift`` takes precedence over it. Keep that
    absolute point for Rust to resolve against activation-effective target layout;
    leave generic timing/rate options for the shared Rust option resolver.
    """

    if not isinstance(target, (_base.Mobject, _compat.Group)):
        raise TypeError("FadeIn/FadeOut target must be a Mobject or Group")

    animation_kwargs = dict(kwargs)
    shift = animation_kwargs.pop("shift", None)
    target_position = animation_kwargs.pop("target_position", None)
    scale_factor = float(animation_kwargs.pop("scale", 1.0))
    if not math.isfinite(scale_factor):
        raise ValueError("fade scale must be finite")

    point_target = False
    point = None
    if shift is not None:
        shift_vector = _base._as_vec2(shift)
    elif target_position is not None:
        if isinstance(target_position, (_base.Mobject, _compat.Group)):
            point = target_position.get_center()
        else:
            point = _base._as_vec2(target_position)
        # Rust resolves the point against the activation-effective target layout.
        shift_vector = _base.ORIGIN
        point_target = True
    else:
        shift_vector = _base.ORIGIN

    return animation_kwargs, shift_vector, scale_factor, point_target, point


def _store_fade_options(
    animation: object,
    *,
    shift_vector: _base.Vec2,
    scale_factor: float,
    point_target: bool,
    point: _base.Vec2 | None,
) -> None:
    object.__setattr__(animation, "_fade_shift_vector", shift_vector)
    object.__setattr__(animation, "_fade_scale_factor", scale_factor)
    object.__setattr__(animation, "_fade_point_target", point_target)
    object.__setattr__(animation, "_fade_point", point)


class Transform(_ORIGINAL_TRANSFORM):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class Indicate:
    """Inert ManimCE ``Indicate`` request for shared Rust semantic playback."""

    def __init__(
        self,
        mobject: object,
        scale_factor: float = 1.2,
        color: _base.Color = _PURE_YELLOW,
        rate_func: object = _rate_functions.there_and_back,
        **kwargs: Any,
    ) -> None:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Indicate target must be a Mobject or Group")
        factor = float(scale_factor)
        if not math.isfinite(factor):
            raise ValueError("Indicate scale_factor must be finite")

        self.mobject = mobject
        self.scale_factor = factor
        self.color = color
        animation_kwargs = dict(kwargs)
        animation_kwargs["rate_func"] = rate_func
        _store_animation_args(self, animation_kwargs)


class ReplacementTransform(_ORIGINAL_REPLACEMENT_TRANSFORM):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class TransformFromCopy(_ORIGINAL_TRANSFORM_FROM_COPY):
    def __init__(
        self,
        source: object,
        target: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(source, target, key)
        _store_animation_args(self, kwargs)


class TransformMatchingShapes(_ORIGINAL_TRANSFORM_MATCHING_SHAPES):
    def __init__(
        self,
        sources: object,
        targets: object,
        key: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(sources, targets, key)
        _store_animation_args(self, kwargs)


class Create(_ORIGINAL_CREATE):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        super().__init__(target, key)
        _store_animation_args(self, kwargs)


class Uncreate(_ORIGINAL_UNCREATE):
    def __init__(
        self,
        target: object,
        key: str | None = None,
        reverse_rate_function: bool = True,
        remover: bool = True,
        **kwargs: Any,
    ) -> None:
        super().__init__(target, key, bool(reverse_rate_function), bool(remover))
        _store_animation_args(self, kwargs)


class FadeIn(_ORIGINAL_FADE_IN):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        animation_kwargs, shift_vector, scale_factor, point_target, point = (
            _fade_authoring_options(target, kwargs)
        )
        super().__init__(target, key)
        _store_animation_args(self, animation_kwargs)
        _store_fade_options(
            self,
            shift_vector=shift_vector,
            scale_factor=scale_factor,
            point_target=point_target,
            point=point,
        )


class FadeOut(_ORIGINAL_FADE_OUT):
    def __init__(self, target: object, key: str | None = None, **kwargs: Any) -> None:
        animation_kwargs, shift_vector, scale_factor, point_target, point = (
            _fade_authoring_options(target, kwargs)
        )
        super().__init__(target, key)
        _store_animation_args(self, animation_kwargs)
        _store_fade_options(
            self,
            shift_vector=shift_vector,
            scale_factor=scale_factor,
            point_target=point_target,
            point=point,
        )


# Replace only the public compatibility classes. Their frozen Noon bases remain the
# low-level representation and existing code using isinstance(..., noon.Create/etc.)
# continues to work because the module globals now point at these subclasses.
for _name, _value in {
    "Transform": Transform,
    "Indicate": Indicate,
    "ReplacementTransform": ReplacementTransform,
    "TransformFromCopy": TransformFromCopy,
    "TransformMatchingShapes": TransformMatchingShapes,
    "Create": Create,
    "Uncreate": Uncreate,
    "FadeIn": FadeIn,
    "FadeOut": FadeOut,
}.items():
    setattr(_base, _name, _value)

if "Indicate" not in _base.__all__:
    _base.__all__.append("Indicate")


class _AnimateBuilderMixin:
    """Mirror Manim's callable/chained ``_AnimationBuilder`` contract."""

    source: object
    target: object
    mobject: object
    anim_args: dict[str, Any]
    cannot_pass_args: bool
    is_chaining: bool

    def _initialize_builder(self, source: object) -> None:
        self.source = source
        self.mobject = source
        target_factory = getattr(source, "_copy_for_animate_target", None)
        self.target = (
            target_factory()
            if target_factory is not None
            else source.copy()  # type: ignore[attr-defined]
        )
        self.anim_args = {}
        self.cannot_pass_args = False
        self.is_chaining = False

    def __call__(self, **kwargs: Any):
        if self.cannot_pass_args:
            raise ValueError(
                "Animation arguments must be passed before accessing methods and can only be passed once"
            )
        self.anim_args = dict(kwargs)
        self.cannot_pass_args = True
        return self

    def __getattr__(self, name: str) -> Callable[..., Any]:
        if name.startswith("_"):
            raise AttributeError(name)
        target_attribute = getattr(self.target, name)
        if not callable(target_attribute):
            raise AttributeError(f"{name} is not an animatable method")

        # Manim prevents animation arguments from being supplied after the first
        # method is accessed, even before the returned method proxy is invoked.
        self.is_chaining = True
        self.cannot_pass_args = True

        def invoke(*args: Any, **kwargs: Any):
            result = target_attribute(*args, **kwargs)
            if result is not None and result is not self.target:
                raise TypeError(
                    f"animate.{name} must be a mutating method returning self or None"
                )
            return self

        return invoke


class _AlignedAnimationBuilder(_AnimateBuilderMixin):
    def __init__(self, source: _base.Mobject) -> None:
        # Unlike the old Noon builder, Manim allows ``self.play(Circle().animate...)``.
        # Binding happens when Scene.play compiles the animation.
        self._initialize_builder(source)


class _AlignedGroupAnimationBuilder(_AnimateBuilderMixin):
    def __init__(self, source: _compat.Group) -> None:
        self._initialize_builder(source)


# Both properties in _manim_compat resolve these globals at access time.
_compat._CompatAnimationBuilder = _AlignedAnimationBuilder
_compat._GroupAnimationBuilder = _AlignedGroupAnimationBuilder
