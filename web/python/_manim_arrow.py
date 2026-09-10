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


def _numeric_endpoint(name: str, value: object) -> _base.Vec2:
    if isinstance(value, (_base.Mobject, _compat.Group)):
        raise NotImplementedError(
            f"{name} Mobject endpoints require the shared Rust boundary-point constructor"
        )
    return _base._as_vec2(value)


def _constructor_options(options: object, kwargs: dict[str, Any]) -> None:
    values = dict(kwargs)
    allowed = {
        "position",
        "rotation",
        "scale",
        "stroke_width",
        "stroke_width_mode",
        "stroke_join",
        "stroke_cap",
        "opacity",
        "z_index",
    }
    unknown = sorted(set(values) - allowed)
    if unknown:
        raise TypeError(f"unsupported Arrow constructor option(s): {', '.join(unknown)}")

    if "z_index" in values:
        engine_call(options.setZIndex, _ir._finite_number("z_index", values["z_index"]))
    if "position" in values:
        point = _ir._vec2("position", values["position"])
        engine_call(options.setTranslation, point["x"], point["y"])
    if "rotation" in values:
        engine_call(options.setRotation, _ir._finite_number("rotation", values["rotation"]))
    if "scale" in values:
        scale = _ir._vec2("scale", values["scale"])
        engine_call(options.setScale, scale["x"], scale["y"])
    if "stroke_width" in values:
        engine_call(options.setStrokeWidth, _compat._manim_stroke_width(values["stroke_width"]))
    if "stroke_width_mode" in values:
        engine_call(options.setStrokeWidthMode, _ir._stroke_width_mode(values["stroke_width_mode"]))
    if "stroke_join" in values:
        engine_call(options.setStrokeJoin, _ir._stroke_join(values["stroke_join"]))
    if "stroke_cap" in values:
        engine_call(options.setStrokeCap, _ir._stroke_cap(values["stroke_cap"]))
    if "opacity" in values:
        engine_call(options.setObjectOpacity, _compat._opacity("opacity", values["opacity"]))


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
    members = [shaft, end_tip]
    if start_tip is not None:
        members.append(start_tip)
    self._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }


def _create(self: "Arrow", options: object, *, color: object | None, kwargs: dict[str, Any]) -> None:
    if _create_arrow_handle is None:
        raise RuntimeError("Arrow construction requires the shared Rust authoring host")
    if _shared._live_constructor_context("Arrow") is not None:
        raise NotImplementedError(
            "live Arrow construction requires shared retained-family publication support"
        )
    if color is not None:
        parsed = _compat._as_color("color", color)
        engine_call(options.setColor, parsed.red, parsed.green, parsed.blue, parsed.alpha)
    _constructor_options(options, kwargs)
    created = engine_call(_create_arrow_handle, options)
    _attach_arrow_family(self, created)


class Arrow(_compat.Group):
    """Straight Manim Arrow backed by one shared Rust semantic family."""

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        buff: float = 0.25,
        path_arc: float | None = None,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
        preserve_tip_size_when_scaling: bool = True,
        normal_vector: object = (0.0, 0.0, 1.0),
        use_rectangular_stem: bool = False,
        tip_shape: object | None = None,
        *,
        tip_length: float = 0.35,
        color: object | None = None,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("Arrow construction requires the shared Rust authoring host")
        if path_arc not in (None, 0, 0.0):
            raise NotImplementedError("curved Arrow path_arc is not part of the straight #77 batch")
        if not preserve_tip_size_when_scaling:
            raise NotImplementedError("preserve_tip_size_when_scaling=False is not yet supported")
        if use_rectangular_stem:
            raise NotImplementedError("rectangular Arrow stems are not part of the straight #77 batch")
        if tip_shape is not None:
            raise NotImplementedError("custom Arrow tip classes require shared tip-shape semantics")
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

        start_point = _numeric_endpoint("start", start)
        end_point = _numeric_endpoint("end", end)
        options = engine_call(
            _arrow_options.arrow,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
        )
        engine_call(options.setBuff, _ir._finite_number("buff", buff))
        engine_call(options.setTipLength, _ir._finite_number("tip_length", tip_length))
        engine_call(
            options.setMaxTipLengthToLengthRatio,
            _ir._finite_number(
                "max_tip_length_to_length_ratio", max_tip_length_to_length_ratio
            ),
        )
        # Rust stores scene-space stroke widths, so only this public Manim stroke
        # ratio crosses the facade with Cairo's documented 0.01 conversion.
        stroke_ratio = _ir._finite_number(
            "max_stroke_width_to_length_ratio", max_stroke_width_to_length_ratio
        )
        engine_call(
            options.setMaxStrokeWidthToLengthRatio,
            stroke_ratio * _compat.MANIM_CAIRO_LINE_WIDTH_MULTIPLE,
        )
        _create(self, options, color=color, kwargs=kwargs)

    def get_start(self) -> _base.Vec2:
        from _manim_path_queries import endpoint

        return endpoint(self.start_tip if self.start_tip is not None else self._shaft, False)

    def get_end(self) -> _base.Vec2:
        from _manim_path_queries import endpoint

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
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("Vector construction requires the shared Rust authoring host")
        direction = _numeric_endpoint("direction", direction)
        options = engine_call(_arrow_options.vector, direction.x, direction.y)
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
        _create(self, options, color=color, kwargs=kwargs)


class DoubleArrow(Arrow):
    """Straight Arrow with shared retained tips at both endpoints."""

    def __init__(
        self,
        start: object = _base.LEFT,
        end: object = _base.RIGHT,
        buff: float = 0.25,
        *,
        color: object | None = None,
        tip_length: float = 0.35,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None:
            raise RuntimeError("DoubleArrow construction requires the shared Rust authoring host")
        start_point = _numeric_endpoint("start", start)
        end_point = _numeric_endpoint("end", end)
        options = engine_call(
            _arrow_options.doubleArrow,
            start_point.x,
            start_point.y,
            end_point.x,
            end_point.y,
        )
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
        _create(self, options, color=color, kwargs=kwargs)


__all__ = ["Arrow", "Vector", "DoubleArrow"]
