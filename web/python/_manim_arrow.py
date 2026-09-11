"""Thin Manim-compatible Arrow families over shared Rust semantics."""

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

try:
    from js import noonAuthoringArrowFromMobject as _arrow_from_mobject
    from js import noonAuthoringArrowFromMobjects as _arrow_from_mobjects
    from js import noonAuthoringArrowToMobject as _arrow_to_mobject
    from js import noonAuthoringDoubleArrowFromMobject as _double_arrow_from_mobject
    from js import noonAuthoringDoubleArrowFromMobjects as _double_arrow_from_mobjects
    from js import noonAuthoringDoubleArrowToMobject as _double_arrow_to_mobject
except ImportError:  # Normal worker hosts expose these on the typed Arrow options bridge.
    _arrow_from_mobject = getattr(_arrow_options, "arrowFromMobject", None)
    _arrow_from_mobjects = getattr(_arrow_options, "arrowFromMobjects", None)
    _arrow_to_mobject = getattr(_arrow_options, "arrowToMobject", None)
    _double_arrow_from_mobject = getattr(
        _arrow_options, "doubleArrowFromMobject", None
    )
    _double_arrow_from_mobjects = getattr(
        _arrow_options, "doubleArrowFromMobjects", None
    )
    _double_arrow_to_mobject = getattr(_arrow_options, "doubleArrowToMobject", None)


_ARROW_CONSTRUCTOR_OPTIONS = frozenset(
    {
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
)

_FIELD_QUERY_METHODS = {
    "startX": "vectorStartX",
    "startY": "vectorStartY",
    "endX": "vectorEndX",
    "endY": "vectorEndY",
    "vectorX": "vectorVectorX",
    "vectorY": "vectorVectorY",
    "length": "vectorLength",
    "angle": "vectorAngle",
    "unitVectorX": "vectorUnitVectorX",
    "unitVectorY": "vectorUnitVectorY",
}


def _numeric_endpoint(name: str, value: object) -> _base.Vec2:
    if isinstance(value, (_base.Mobject, _compat.Group)):
        raise NotImplementedError(
            f"{name} Mobject endpoints require the shared Rust boundary-point constructor"
        )
    return _base._as_vec2(value)


def _mobject_endpoint_handle(name: str, value: object):
    if isinstance(value, _compat.Group):
        raise NotImplementedError(
            f"{name} Group endpoints require shared family boundary-point semantics"
        )
    if not isinstance(value, _base.Mobject):
        return None
    handle = _shared._handle_for(value)
    if handle is None:
        raise RuntimeError(f"{name} Mobject endpoint requires a current shared Rust handle")
    return handle


def _arrow_endpoint_options(start: object, end: object, *, double_arrow: bool):
    start_handle = _mobject_endpoint_handle("start", start)
    end_handle = _mobject_endpoint_handle("end", end)

    if start_handle is not None and end_handle is not None:
        operation = (
            _double_arrow_from_mobjects if double_arrow else _arrow_from_mobjects
        )
        if operation is None:
            raise RuntimeError("Arrow Mobject endpoints require the shared Rust boundary host")
        return engine_call(operation, start_handle, end_handle, operation="Arrow.boundaryEndpoints")

    if start_handle is not None:
        end_point = _numeric_endpoint("end", end)
        operation = (
            _double_arrow_from_mobject if double_arrow else _arrow_from_mobject
        )
        if operation is None:
            raise RuntimeError("Arrow Mobject endpoints require the shared Rust boundary host")
        return engine_call(
            operation,
            start_handle,
            end_point.x,
            end_point.y,
            operation="Arrow.boundaryEndpoints",
        )

    if end_handle is not None:
        start_point = _numeric_endpoint("start", start)
        operation = _double_arrow_to_mobject if double_arrow else _arrow_to_mobject
        if operation is None:
            raise RuntimeError("Arrow Mobject endpoints require the shared Rust boundary host")
        return engine_call(
            operation,
            start_point.x,
            start_point.y,
            end_handle,
            operation="Arrow.boundaryEndpoints",
        )

    start_point = _numeric_endpoint("start", start)
    end_point = _numeric_endpoint("end", end)
    factory = _arrow_options.doubleArrow if double_arrow else _arrow_options.arrow
    return engine_call(
        factory,
        start_point.x,
        start_point.y,
        end_point.x,
        end_point.y,
    )


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

    # Retain the aggregate Rust handle as the query/mutation capability. It owns no
    # parallel Python semantic state: policy and geometry live on the same retained
    # semantic shaft/tip leaves exposed below.
    self._semantic_arrow_handle = created
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


def _arrow_scalar(self: "Arrow", method: str) -> float:
    handle = getattr(self, "_semantic_arrow_handle", None)
    index = getattr(self, "_semantic_arrow_index", None)
    if handle is None:
        raise RuntimeError("Arrow observation requires the shared Rust authoring host")
    operation_name = method if index is None else _FIELD_QUERY_METHODS[method]
    operation = getattr(handle, operation_name, None)
    if operation is None:
        raise RuntimeError("Arrow observation requires the shared Rust authoring host")
    if index is None:
        return float(engine_call(operation, operation=f"Arrow.{method}"))
    return float(engine_call(operation, index, operation=f"Arrow.{method}"))


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
        *,
        stroke_width: float = 6.0,
        buff: float = 0.25,
        path_arc: float | None = 0.0,
        max_tip_length_to_length_ratio: float = 0.25,
        max_stroke_width_to_length_ratio: float = 5.0,
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
        options = _arrow_endpoint_options(start, end, double_arrow=False)
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
        if kwargs:
            unknown = ", ".join(sorted(kwargs))
            raise NotImplementedError(
                f"Arrow.scale pivot option(s) are not yet shared Rust semantics: {unknown}"
            )
        if _shared._group_live_layout_context(self) is not None:
            raise NotImplementedError(
                "live Arrow.scale requires shared dependent publication support"
            )
        handle = getattr(self, "_semantic_arrow_handle", None)
        operation = getattr(handle, "scale", None) if handle is not None else None
        if operation is None:
            raise RuntimeError("Arrow.scale requires the shared Rust authoring host")
        engine_call(
            operation,
            _ir._finite_number("factor", factor),
            bool(scale_tips),
            operation="Arrow.scale",
        )
        return self

    def get_start(self) -> _base.Vec2:
        return _base.Vec2(
            _arrow_scalar(self, "startX"),
            _arrow_scalar(self, "startY"),
        )

    def get_end(self) -> _base.Vec2:
        return _base.Vec2(
            _arrow_scalar(self, "endX"),
            _arrow_scalar(self, "endY"),
        )

    def get_vector(self) -> _base.Vec2:
        return _base.Vec2(
            _arrow_scalar(self, "vectorX"),
            _arrow_scalar(self, "vectorY"),
        )

    def get_length(self) -> float:
        return _arrow_scalar(self, "length")

    def get_unit_vector(self) -> _base.Vec2:
        return _base.Vec2(
            _arrow_scalar(self, "unitVectorX"),
            _arrow_scalar(self, "unitVectorY"),
        )

    def get_angle(self) -> float:
        return _arrow_scalar(self, "angle")

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
        *,
        stroke_width: float = 6.0,
        buff: float = 0.25,
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
        options = _arrow_endpoint_options(start, end, double_arrow=True)
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


_DEFAULT_VECTOR_FIELD_LENGTH = object()


def _vector_field_range(name: str, value: object) -> tuple[list[float], list[float]]:
    if value is None:
        raise NotImplementedError(
            f"ArrowVectorField {name}=None requires shared frame-derived default ranges"
        )
    try:
        values = [float(component) for component in value]  # type: ignore[arg-type]
    except (TypeError, ValueError) as error:
        raise TypeError(f"{name} must contain two or three numeric values") from error
    if len(values) == 2:
        values.append(0.5)
    elif len(values) != 3:
        raise ValueError(f"{name} must contain [min, max] or [min, max, step]")
    if any(not _base.math.isfinite(component) for component in values):
        raise ValueError(f"{name} must contain finite values")
    nominal = values.copy()
    public = values.copy()
    public[1] += public[2]
    return nominal, public


def _default_vector_field_length(norm: float) -> float:
    return 0.45 / (1.0 + _base.math.exp(-float(norm)))


def _attach_vector_field_member(created: object, index: int) -> Vector:
    wrapper = object.__new__(Vector)
    family = engine_call(created.vectorFamily, index)
    shaft = _leaf(engine_call(created.vectorShaft, index), _compat.Line)
    end_tip = _leaf(engine_call(created.vectorEndTip, index))
    wrapper._semantic_arrow_handle = created
    wrapper._semantic_arrow_index = index
    wrapper._semantic_family_handle = family
    wrapper._shaft = shaft
    wrapper.tip = end_tip
    wrapper.start_tip = None
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in (shaft, end_tip)
    }
    return wrapper


class ArrowVectorField(_compat.VGroup):
    """Static 2D ArrowVectorField prepared at Rust-owned Manim sample points."""

    def __init__(
        self,
        func,
        color=None,
        color_scheme=None,
        min_color_scheme_value: float = 0,
        max_color_scheme_value: float = 2,
        colors=None,
        *,
        x_range=None,
        y_range=None,
        z_range=None,
        three_dimensions: bool = False,
        length_func=_DEFAULT_VECTOR_FIELD_LENGTH,
        opacity: float = 1.0,
        vector_config=None,
        **kwargs: Any,
    ) -> None:
        if _arrow_options is None or _create_arrow_handle is None:
            raise RuntimeError("ArrowVectorField construction requires the shared Rust authoring host")
        if _shared._live_constructor_context("ArrowVectorField") is not None:
            raise NotImplementedError(
                "live ArrowVectorField construction requires shared retained-family publication support"
            )
        if not callable(func):
            raise TypeError("ArrowVectorField func must be callable")
        if color is not None:
            raise NotImplementedError("ArrowVectorField single-color mode is not part of this B5 slice")
        if color_scheme is not None:
            raise NotImplementedError("custom ArrowVectorField color_scheme is not part of this B5 slice")
        if float(min_color_scheme_value) != 0.0 or float(max_color_scheme_value) != 2.0:
            raise NotImplementedError("custom ArrowVectorField color-scheme bounds are not part of this B5 slice")
        if colors is not None:
            raise NotImplementedError("custom ArrowVectorField color lists are not part of this B5 slice")
        if z_range is not None or three_dimensions:
            raise NotImplementedError("3D ArrowVectorField requires the Phase B6/C 3D field contract")
        opacity_value = _ir._finite_number("opacity", opacity)
        if opacity_value != 1.0:
            raise NotImplementedError("ArrowVectorField opacity other than 1.0 is not part of this B5 slice")
        if vector_config not in (None, {}):
            raise NotImplementedError("ArrowVectorField vector_config is not part of this B5 slice")
        if kwargs:
            raise TypeError(
                "unsupported ArrowVectorField constructor option(s): "
                + ", ".join(sorted(kwargs))
            )

        x_nominal, x_public = _vector_field_range("x_range", x_range)
        y_nominal, y_public = _vector_field_range("y_range", y_range)
        custom_length = length_func is not _DEFAULT_VECTOR_FIELD_LENGTH
        if custom_length and not callable(length_func):
            raise TypeError("ArrowVectorField length_func must be callable")

        draft = engine_call(
            _arrow_options.vectorField,
            x_nominal[0],
            x_nominal[1],
            x_nominal[2],
            y_nominal[0],
            y_nominal[1],
            y_nominal[2],
            custom_length,
        )
        try:
            sample_count = int(draft.sampleCount)
            for index in range(sample_count):
                x = float(engine_call(draft.sampleX, index))
                y = float(engine_call(draft.sampleY, index))
                raw = _base._as_vec2(func((x, y, 0.0)))
                engine_call(draft.setVector, index, raw.x, raw.y)
                if custom_length:
                    norm = raw.length()
                    if norm != 0.0:
                        display_length = _ir._finite_number(
                            "length_func result", length_func(norm)
                        )
                        engine_call(draft.setDisplayLength, index, display_length)

            created = engine_call(_create_arrow_handle, draft)
        finally:
            release = getattr(draft, "free", None)
            if release is not None:
                try:
                    engine_call(release)
                except Exception:
                    pass

        self._semantic_arrow_handle = created
        self._semantic_family_handle = engine_call(created.family)
        vector_count = int(created.vectorCount)
        vectors = [
            _attach_vector_field_member(created, index)
            for index in range(vector_count)
        ]
        self._semantic_member_wrappers = {
            _shared._family_wrapper_key(member): member for member in vectors
        }

        self.func = func
        self.x_range = x_public
        self.y_range = y_public
        self.z_range = [0.0, 0.5, 0.5]
        self.ranges = [self.x_range, self.y_range, self.z_range]
        self.length_func = (
            length_func if custom_length else _default_vector_field_length
        )
        self.opacity = opacity_value
        self.vector_config = {}
        self.single_color = False


__all__ = ["Arrow", "Vector", "DoubleArrow", "ArrowVectorField"]
