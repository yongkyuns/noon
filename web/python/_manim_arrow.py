"""Thin Manim-compatible Arrow family facade over shared Rust semantics."""

from __future__ import annotations

from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
import _noon_ir as _ir
from _noon_errors import engine_call

try:
    from js import noonAuthoringArrowOptions as _arrow_options
    from js import noonCreateAuthoringArrowHandle as _create_arrow_handle
except ImportError:  # Native CPython tests install explicit bridge fixtures.
    _arrow_options = None
    _create_arrow_handle = None


_ARROW_CONSTRUCTOR_OPTIONS = frozenset(
    {
        "position",
        "rotation",
        "scale",
        "fill",
        "stroke",
        "stroke_width",
        "stroke_width_mode",
        "stroke_join",
        "stroke_cap",
        "opacity",
        "fill_color",
        "stroke_color",
        "fill_opacity",
        "stroke_opacity",
        "z_index",
    }
)


def _numeric_endpoint(name: str, value: object) -> _base.Vec2:
    if isinstance(value, (_base.Mobject, _compat.Group)):
        raise NotImplementedError(
            f"{name} Mobject endpoints require the shared Rust boundary-point constructor"
        )
    return _base._as_vec2(value)


def _apply_constructor_options(options: object, kwargs: dict[str, Any]) -> None:
    """Coerce the supported Python constructor surface, then delegate to Rust."""

    values = dict(kwargs)
    unknown = sorted(set(values) - _ARROW_CONSTRUCTOR_OPTIONS)
    if unknown:
        raise TypeError(f"unsupported Arrow constructor option(s): {', '.join(unknown)}")
    _shared._apply_shared_constructor_options(options, values)


def _apply_arrow_parameters(
    options: object,
    *,
    buff: object,
    tip_length: object,
    max_tip_length_to_length_ratio: object,
    max_stroke_width_to_length_ratio: object,
) -> None:
    """Convert public Manim units; Rust owns every resulting Arrow rule."""

    engine_call(options.setBuff, _ir._finite_number("buff", buff))
    engine_call(options.setTipLength, _ir._finite_number("tip_length", tip_length))
    engine_call(
        options.setMaxTipLengthToLengthRatio,
        _ir._finite_number(
            "max_tip_length_to_length_ratio", max_tip_length_to_length_ratio
        ),
    )
    stroke_ratio = _ir._finite_number(
        "max_stroke_width_to_length_ratio", max_stroke_width_to_length_ratio
    )
    engine_call(
        options.setMaxStrokeWidthToLengthRatio,
        stroke_ratio * _compat.MANIM_CAIRO_LINE_WIDTH_MULTIPLE,
    )


def _leaf(handle: object, cls: type[_base.Mobject] = _compat.VMobject) -> _base.Mobject:
    wrapper = object.__new__(cls)
    _shared._attach_shared_handle(wrapper, handle)
    return wrapper


def _attach_arrow_family(self: "Arrow", created: object) -> None:
    family = engine_call(created.family)
    shaft = _leaf(engine_call(created.shaft), _compat.Line)
    end_tip = _leaf(engine_call(created.endTip))
    start_tip = _leaf(engine_call(created.startTip)) if bool(created.hasStartTip) else None

    self._semantic_family_handle = family
    self._shaft = shaft
    self.tip = end_tip
    self.start_tip = start_tip
    members = [shaft]
    if start_tip is not None:
        members.append(start_tip)
    members.append(end_tip)
    self._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }


def _create(
    self: "Arrow",
    options: object,
    *,
    color: object | None,
    kwargs: dict[str, Any],
) -> None:
    if _create_arrow_handle is None:
        raise RuntimeError("Arrow construction requires the shared Rust authoring host")
    if _shared._live_constructor_context("Arrow") is not None:
        raise NotImplementedError(
            "live Arrow construction requires shared retained-family publication support"
        )
    _apply_constructor_options(options, kwargs)
    if color is not None:
        _shared._apply_constructor_color(options, _compat._as_color("color", color))
    created = engine_call(_create_arrow_handle, options)
    _attach_arrow_family(self, created)


def _validate_straight_arrow_options(
    *,
    path_arc: float | None,
    normal_vector: object,
    tip_shape: object | None,
    tip_style: object | None,
) -> None:
    """Reject ManimCE breadth this straight-Arrow batch does not claim."""

    if path_arc not in (None, 0, 0.0):
        raise NotImplementedError("curved Arrow path_arc is not part of the straight #77 batch")
    if tip_shape is not None:
        raise NotImplementedError("custom Arrow tip classes require shared tip-shape semantics")
    if tip_style not in (None, {}):
        raise NotImplementedError("Arrow tip_style requires shared tip-style semantics")
    try:
        normal = tuple(float(value) for value in normal_vector)  # type: ignore[arg-type]
    except (TypeError, ValueError) as error:
        raise TypeError("normal_vector must be a three-component numeric vector") from error
    if len(normal) != 3 or any(not _base.math.isfinite(value) for value in normal):
        raise ValueError("normal_vector must contain three finite values")
    if not (
        abs(normal[0]) <= 1.0e-12
        and abs(normal[1]) <= 1.0e-12
        and abs(abs(normal[2]) - 1.0) <= 1.0e-12
    ):
        raise NotImplementedError("2D Arrow supports only the +/-z normal")


class Arrow(_compat.Group):
    """Straight Manim Arrow backed by one shared Rust semantic family."""

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        stroke_width: float = 6.0,
        buff: float = 0.25,
        path_arc: float | None = 0.0,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
        *,
        tip_length: float = 0.35,
        normal_vector: object = (0.0, 0.0, 1.0),
        tip_style: dict[str, Any] | None = None,
        tip_shape: object | None = None,
        color: object | None = None,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("Arrow construction requires the shared Rust authoring host")
        _validate_straight_arrow_options(
            path_arc=path_arc,
            normal_vector=normal_vector,
            tip_shape=tip_shape,
            tip_style=tip_style,
        )
        start_point = _numeric_endpoint("start", start)
        end_point = _numeric_endpoint("end", end)
        options = engine_call(
            _arrow_options.arrow,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
        )
        _apply_arrow_parameters(
            options,
            buff=buff,
            tip_length=tip_length,
            max_tip_length_to_length_ratio=max_tip_length_to_length_ratio,
            max_stroke_width_to_length_ratio=max_stroke_width_to_length_ratio,
        )
        constructor_options = dict(kwargs)
        constructor_options["stroke_width"] = stroke_width
        _create(self, options, color=color, kwargs=constructor_options)

    def scale(self, factor: float, scale_tips: bool = False, **kwargs: Any):
        del factor, scale_tips, kwargs
        raise NotImplementedError(
            "Arrow.scale requires shared Rust preserve-tip-size and stroke recapping semantics"
        )

    def get_start(self) -> _base.Vec2:
        from _manim_path_queries import endpoint

        return endpoint(self.start_tip if self.start_tip is not None else self._shaft, False)

    def get_end(self) -> _base.Vec2:
        from _manim_path_queries import endpoint

        # Rust builds the tip path with its public apex as the first retained point.
        return endpoint(self.tip, False)

    def get_tip(self):
        return self.tip

    def has_tip(self) -> bool:
        return self.tip is not None

    def has_start_tip(self) -> bool:
        return self.start_tip is not None

    def get_start_tip(self):
        if self.start_tip is None:
            raise AttributeError("Arrow has no start tip")
        return self.start_tip


class Vector(Arrow):
    """Arrow from ORIGIN to ``direction`` with Manim's default zero buff."""

    def __init__(
        self,
        direction: object = _base.RIGHT,
        buff: float = 0.0,
        *,
        color: object | None = None,
        tip_length: float = 0.35,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
        stroke_width: float = 6.0,
        normal_vector: object = (0.0, 0.0, 1.0),
        tip_style: dict[str, Any] | None = None,
        tip_shape: object | None = None,
        path_arc: float | None = 0.0,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("Vector construction requires the shared Rust authoring host")
        _validate_straight_arrow_options(
            path_arc=path_arc,
            normal_vector=normal_vector,
            tip_shape=tip_shape,
            tip_style=tip_style,
        )
        direction_point = _numeric_endpoint("direction", direction)
        options = engine_call(_arrow_options.vector, direction_point.x, direction_point.y)
        _apply_arrow_parameters(
            options,
            buff=buff,
            tip_length=tip_length,
            max_tip_length_to_length_ratio=max_tip_length_to_length_ratio,
            max_stroke_width_to_length_ratio=max_stroke_width_to_length_ratio,
        )
        constructor_options = dict(kwargs)
        constructor_options["stroke_width"] = stroke_width
        _create(self, options, color=color, kwargs=constructor_options)


class DoubleArrow(Arrow):
    """Straight Arrow with shared retained tips at both endpoints."""

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        stroke_width: float = 6.0,
        buff: float = 0.25,
        *,
        color: object | None = None,
        tip_length: float = 0.35,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
        normal_vector: object = (0.0, 0.0, 1.0),
        tip_style: dict[str, Any] | None = None,
        tip_shape: object | None = None,
        tip_shape_end: object | None = None,
        tip_shape_start: object | None = None,
        path_arc: float | None = 0.0,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("DoubleArrow construction requires the shared Rust authoring host")
        if tip_shape_end is not None or tip_shape_start is not None:
            raise NotImplementedError("custom DoubleArrow tip classes require shared tip-shape semantics")
        _validate_straight_arrow_options(
            path_arc=path_arc,
            normal_vector=normal_vector,
            tip_shape=tip_shape,
            tip_style=tip_style,
        )
        start_point = _numeric_endpoint("start", start)
        end_point = _numeric_endpoint("end", end)
        options = engine_call(
            _arrow_options.doubleArrow,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
        )
        _apply_arrow_parameters(
            options,
            buff=buff,
            tip_length=tip_length,
            max_tip_length_to_length_ratio=max_tip_length_to_length_ratio,
            max_stroke_width_to_length_ratio=max_stroke_width_to_length_ratio,
        )
        constructor_options = dict(kwargs)
        constructor_options["stroke_width"] = stroke_width
        _create(self, options, color=color, kwargs=constructor_options)


__all__ = ["Arrow", "Vector", "DoubleArrow"]
