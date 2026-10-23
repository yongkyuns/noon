"""Scaled coordinate families and preparation-only plots over shared Rust handles.

The 2D Axes wrapper accepts Manim-style two- or three-value ranges and delegates
range normalization, defaults, ticks, and retained filled tips to Rust. The
bounded ThreeDAxes constructor retains its three default tips.
Coordinates may be constructed after ordinary plays/waits;
add late coordinates to the Scene before querying or plotting against them.
Numeric native Text label families must be constructed before the first play/wait.
Existing coordinates remain queryable after plays; curves use ordinary live
geometry publication. Python owns callables/coercion, never coordinate math.
"""
from __future__ import annotations

from contextlib import contextmanager
from collections.abc import Mapping
import math
from operator import index as _index
from typing import NamedTuple

import noon as _base
import _manim_compat as _compat
import _manim_semantic_handles as _shared
from _noon_errors import engine_call

try:
    from js import noonAuthoringCoordinateOptions as _coordinate_options
    from js import noonCreateAuthoringCoordinateHandle as _create_coordinates
    from js import noonAuthoringBarChartOptions as _bar_chart_options
    from js import noonCreateAuthoringBarChart as _create_bar_chart
    from js import noonBarChartLabels as _bar_labels
    from js import noonPlotSamplingPlan as _sampling_plan
    from pyodide.ffi import to_js as _to_js, jsnull as _jsnull
except ImportError:
    _coordinate_options = _create_coordinates = _bar_chart_options = _create_bar_chart = _bar_labels = _sampling_plan = _to_js = _jsnull = None


class _NumberLabel(NamedTuple):
    number: float
    text: str
    point: _base.Vec2


class _TimeSeriesPlan(NamedTuple):
    points: tuple[_base.Vec2, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


class _SynchronizedTimeSeriesPlan(NamedTuple):
    series_points: tuple[tuple[_base.Vec2, ...], ...]
    data_times: tuple[float, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


class _GappedTimeSeriesPlan(NamedTuple):
    series_points: tuple[tuple[_base.Vec2 | None, ...], ...]
    series_segments: tuple[tuple[tuple[_base.Vec2, _base.Vec2] | None, ...], ...]
    data_times: tuple[float, ...]
    cursor_points: tuple[_base.Vec2, ...]
    key_times: tuple[float, ...]
    durations: tuple[float, ...]
    run_time: float


def _break_index(value):
    if isinstance(value, bool):
        raise TypeError("break-after indices must be integers, not booleans")
    return _index(value)


def _point_pairs(values):
    """Project an already validated Rust vector, without coordinate calculation."""
    return tuple(_base.Vec2(float(values[i]), float(values[i + 1]))
                 for i in range(0, len(values), 2))


@contextmanager
def _owned(value):
    """Release disposable WASM observations/plans, including callback failures."""
    try:
        yield value
    finally:
        value.free()


def _array(values):
    if _to_js is None:
        raise RuntimeError("plotting requires the shared Rust authoring host")
    return _to_js([float(value) for value in values])


def _outside_callback():
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError(
            "coordinate queries and plot construction inside callbacks require pinned coordinate reads"
        )


def _coordinate_context(shafts):
    _outside_callback()
    contexts = [context for shaft in shafts
                if (context := _shared._live_mutation_context(shaft)) is not None]
    if contexts and any(context is not contexts[0] for context in contexts[1:]):
        raise RuntimeError("coordinate shafts belong to different execution contexts")
    context = contexts[0] if contexts else _shared._live_constructor_context("coordinate query")
    if context is not None and not all(_shared._is_bound(shaft) for shaft in shafts):
        raise NotImplementedError(
            "add live coordinates to the Scene before querying or plotting; "
            "detached live coordinate reads are not yet exposed by the Python context"
        )
    return context


def _family(wrapper, handle, members):
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }
    return wrapper


def _coordinate_family(wrapper, handle, members):
    """Seed known coordinate wrappers, then reconcile Rust-owned membership."""
    wrapper._semantic_member_wrappers = {
        _shared._family_wrapper_key(member): member for member in members
    }
    return _shared._attach_shared_family(wrapper, handle, leaf_type=_compat.VMobject)


def _leaf(handle, kind=_compat.Line):
    wrapper = object.__new__(kind)
    _shared._attach_shared_handle(wrapper, handle)
    return wrapper


def _attach_number_line(wrapper, handle, *, tick_size=0.1, include_ticks=None,
                        include_tip=False, exclude_origin_tick=False):
    shaft = _leaf(engine_call(handle.coordinateShaft))
    ticks = _family(
        object.__new__(_compat.VGroup),
        engine_call(handle.coordinateTicks),
        [_leaf(tick) for tick in engine_call(handle.coordinateTickObjects)],
    )
    members = [shaft, ticks]
    if include_tip:
        wrapper.tip = _leaf(engine_call(handle.coordinateTip), _compat.VMobject)
        members.append(wrapper.tip)
    wrapper.tick_size = tick_size
    if include_ticks is not None:
        wrapper.include_ticks = include_ticks
    wrapper.include_tip = include_tip
    wrapper.exclude_origin_tick = exclude_origin_tick
    return _coordinate_family(wrapper, handle, members)


def _coordinate_style(options, color, kwargs):
    values = dict(kwargs)
    allowed = {"stroke_width", "stroke_opacity", "opacity"}
    unknown = sorted(set(values) - allowed)
    if unknown:
        raise TypeError("unsupported coordinate option(s): " + ", ".join(unknown))
    _shared._apply_shared_constructor_options(options, values)
    _shared._apply_constructor_color(options, color)


def _coordinate_constructor_context():
    _outside_callback()
    if _create_coordinates is None:
        raise RuntimeError("coordinates require the shared Rust authoring host")
    return _shared._live_constructor_context("coordinates")


_THREE_D_AXIS_CONFIG_KEYS = {
    "include_tip", "include_ticks", "tick_size", "exclude_origin_tick",
    "tip_length", "color", "stroke_width", "stroke_opacity", "opacity",
}
_DEFAULT_NUMBER_LINE_TIP_SIZE = 0.35


class LinearBase:
    """Linear NumberLine scale description; mapping is evaluated in Rust."""

    custom_labels = False

    def __init__(self, scale_factor=1.0):
        scale_factor = _finite_axis_option(scale_factor, "scale_factor")
        if scale_factor == 0:
            raise ValueError("LinearBase scale_factor must be non-zero")
        self.scale_factor = scale_factor

    def function(self, value):
        return _map_scale_values(value, lambda item: self.scale_factor * item)

    def inverse_function(self, value):
        return _map_scale_values(value, lambda item: item / self.scale_factor)


class LogBase:
    """Logarithmic NumberLine scale description; mapping is evaluated in Rust."""

    def __init__(self, base=10, custom_labels=True):
        base = _finite_axis_option(base, "base")
        if base <= 0 or base == 1:
            raise ValueError("LogBase base must be positive and not equal to one")
        if not isinstance(custom_labels, bool):
            raise TypeError("custom_labels must be a bool")
        self.base = base
        self.custom_labels = custom_labels

    def function(self, value):
        return _map_scale_values(value, lambda item: self.base ** item)

    def inverse_function(self, value):
        def inverse(item):
            if item <= 0:
                raise ValueError("logarithmic scale values must be positive")
            return math.log(item, self.base)
        return _map_scale_values(value, inverse)


def _map_scale_values(value, function):
    try:
        scalar = float(value)
    except (TypeError, ValueError):
        try:
            return [function(float(item)) for item in value]
        except TypeError:
            raise TypeError("scale value must be a number or iterable of numbers") from None
    return function(scalar)


def _number_line_scale(scaling):
    if scaling is None:
        return "linear", 1.0, False
    if isinstance(scaling, LinearBase):
        return "linear", scaling.scale_factor, False
    if isinstance(scaling, LogBase):
        return "log", scaling.base, scaling.custom_labels
    if type(scaling).__name__ == "LinearBase" and hasattr(scaling, "scale_factor"):
        factor = _finite_axis_option(scaling.scale_factor, "scale_factor")
        if factor == 0:
            raise ValueError("LinearBase scale_factor must be non-zero")
        return "linear", factor, bool(getattr(scaling, "custom_labels", False))
    if type(scaling).__name__ == "LogBase" and hasattr(scaling, "base"):
        base = _finite_axis_option(scaling.base, "base")
        if base <= 0 or base == 1:
            raise ValueError("LogBase base must be positive and not equal to one")
        return "log", base, bool(getattr(scaling, "custom_labels", True))
    raise NotImplementedError("NumberLine scaling supports LinearBase and LogBase")


_AXES_CONFIG_KEYS = {
    "scaling", "include_numbers", "numbers_to_include", "numbers_to_exclude",
    "font_size", "label_direction", "line_to_number_buff", "decimal_number_config",
}

_UNSUPPORTED_LOG_LABELS = "generated LogBase exponent labels are not yet supported"

_DECIMAL_NUMBER_FORMAT_KEYS = frozenset((
    "num_decimal_places", "include_sign", "group_with_commas", "show_ellipsis", "unit",
))


def _coordinate_config(config, name, allowed_keys):
    if config is None:
        return {}
    if not isinstance(config, Mapping):
        raise TypeError(f"{name} must be a mapping")
    if any(not isinstance(key, str) for key in config):
        raise TypeError(f"{name} keys must be strings")
    unknown = sorted(set(config) - allowed_keys)
    if unknown:
        raise NotImplementedError(
            f"unsupported {name} option(s): " + ", ".join(unknown)
        )
    return dict(config)


def _axes_scalings(axis_config, x_axis_config, y_axis_config):
    common = _coordinate_config(axis_config, "axis_config", _AXES_CONFIG_KEYS)
    x_config = {**common, **_coordinate_config(x_axis_config, "x_axis_config", _AXES_CONFIG_KEYS)}
    y_config = {**common, **_coordinate_config(y_axis_config, "y_axis_config", _AXES_CONFIG_KEYS)}
    try:
        x_scale = _number_line_scale(x_config.get("scaling"))
        y_scale = _number_line_scale(y_config.get("scaling"))
    except NotImplementedError as error:
        raise NotImplementedError(
            "Axes scaling supports LinearBase and LogBase"
        ) from error
    return (
        _axes_label_options(x_config, x_scale, "x_axis_config", _base.DOWN), x_scale,
        _axes_label_options(y_config, y_scale, "y_axis_config", _base.LEFT), y_scale,
    )


def _number_label_style(font_size, direction, buff, decimal_config, name):
    font_size = _finite_axis_option(font_size, f"{name}.font_size")
    if font_size <= 0:
        raise ValueError(f"{name}.font_size must be positive")
    planar = _base._as_vec2(direction)
    direction = _base._Direction3(planar.x, planar.y, 0.0)
    buff = _finite_axis_option(buff, f"{name}.line_to_number_buff")
    infer_precision = decimal_config is None
    normalized = _decimal_number_options(decimal_config, f"{name}.decimal_number_config")
    precision = None if infer_precision else normalized.get("num_decimal_places", 2)
    return font_size, direction, buff, precision


def _axes_label_options(config, scale, name, default_direction):
    include_numbers = config.get("include_numbers", False)
    if not isinstance(include_numbers, bool):
        raise TypeError(f"{name}.include_numbers must be a boolean")
    numbers_to_include = _axes_number_values(
        config.get("numbers_to_include"), f"{name}.numbers_to_include", None
    )
    numbers_to_exclude = _axes_number_values(
        config.get("numbers_to_exclude"), f"{name}.numbers_to_exclude", [0.0]
    )
    font_size, direction, buff, decimal_places = _number_label_style(
        config.get("font_size", 36), config.get("label_direction", default_direction),
        config.get("line_to_number_buff", _base.MED_SMALL_BUFF),
        config.get("decimal_number_config"), name,
    )
    decimal_config = _decimal_number_options(
        config.get("decimal_number_config"), f"{name}.decimal_number_config",
    )
    custom_log_labels = scale[0] == "log" and scale[2]
    if custom_log_labels and (include_numbers or numbers_to_include is not None):
        raise NotImplementedError(_UNSUPPORTED_LOG_LABELS)

    return {
        "scaling": config.get("scaling"),
        "include_numbers": include_numbers,
        "numbers_to_include": numbers_to_include,
        "numbers_to_exclude": numbers_to_exclude,
        "font_size": font_size,
        "label_direction": direction,
        "line_to_number_buff": buff,
        "decimal_places": decimal_places,
        "decimal_number_config": decimal_config,
    }


def _decimal_number_options(value, name):
    if value is None:
        return {}
    if not isinstance(value, Mapping):
        raise TypeError(f"{name} must be a mapping")
    if any(not isinstance(key, str) for key in value):
        raise TypeError(f"{name} keys must be strings")
    unknown = sorted(set(value) - _DECIMAL_NUMBER_FORMAT_KEYS)
    if unknown:
        raise NotImplementedError(
            f"unsupported {name} option(s): " + ", ".join(unknown)
        )
    normalized = dict(value)
    if "num_decimal_places" in normalized:
        places = normalized["num_decimal_places"]
        if not isinstance(places, int) or isinstance(places, bool):
            raise TypeError(f"{name}.num_decimal_places must be an integer")
        if not 0 <= places <= 12:
            raise ValueError(f"{name}.num_decimal_places must be between 0 and 12")
    for key in ("include_sign", "group_with_commas", "show_ellipsis"):
        if key in normalized and not isinstance(normalized[key], bool):
            raise TypeError(f"{name}.{key} must be a boolean")
    unit = normalized.get("unit")
    if unit is not None and not isinstance(unit, str):
        raise TypeError(f"{name}.unit must be a string or None")
    return normalized


def _axes_number_values(values, name, default):
    if values is None:
        return default
    if isinstance(values, (str, bytes, Mapping)):
        raise TypeError(f"{name} must be an iterable of finite numbers")
    try:
        values = tuple(values)
    except TypeError as error:
        raise TypeError(f"{name} must be an iterable of finite numbers") from error
    return tuple(_base._ir._finite_number(name, value) for value in values)


def _configure_axes_numeric_labels(axis, config, range_values):
    step = range_values[2] if len(range_values) == 3 else 1
    places = config["decimal_places"]
    if places is None:
        places = _decimal_places_from_step(step)
    axis.include_numbers = config["include_numbers"]
    axis.numbers_to_include = config["numbers_to_include"]
    axis.numbers_to_exclude = config["numbers_to_exclude"]
    axis.font_size = config["font_size"]
    axis.label_direction = config["label_direction"]
    axis.line_to_number_buff = config["line_to_number_buff"]
    axis.number_label_decimal_places = places
    decimal_config = dict(config.get("decimal_number_config", {}))
    decimal_config.setdefault("num_decimal_places", places)
    axis.decimal_number_config = decimal_config
    if axis.include_numbers or axis.numbers_to_include is not None:
        axis.add_numbers(
            x_values=axis.numbers_to_include,
            excluding=axis.numbers_to_exclude,
            font_size=axis.font_size, direction=axis.label_direction,
            buff=axis.line_to_number_buff, decimal_places=places,
        )


def _three_d_axis_config(value, name):
    if value is None:
        return {}
    if not isinstance(value, Mapping):
        raise TypeError(f"{name} must be a mapping")
    unknown = sorted(set(value) - _THREE_D_AXIS_CONFIG_KEYS)
    if unknown:
        raise NotImplementedError(f"unsupported {name} option(s): " + ", ".join(unknown))
    return dict(value)


def _finite_axis_option(value, name, *, nonnegative=False):
    if isinstance(value, bool):
        raise TypeError(f"{name} requires a finite number")
    value = float(value)
    if not math.isfinite(value) or (nonnegative and value < 0):
        raise ValueError(f"{name} must be finite" + (" and nonnegative" if nonnegative else ""))
    if name in ("opacity", "stroke_opacity") and not 0.0 <= value <= 1.0:
        raise ValueError(f"{name} must be between 0 and 1")
    return value


def _decimal_places_from_step(step):
    step_text = str(step)
    return len(step_text.split(".")[-1]) if "." in step_text else 0


def _apply_three_d_axis_config(options, index, config):
    setters = {
        "include_tip": "setAxisTips",
        "include_ticks": "setAxisTicks",
        "exclude_origin_tick": "setAxisExcludeOriginTick",
    }
    for key, method in setters.items():
        if key in config:
            if not isinstance(config[key], bool):
                raise TypeError(f"{key} requires a boolean")
            engine_call(getattr(options, method), index, config[key])
    if "tick_size" in config:
        engine_call(options.setAxisTickSize, index,
                    _finite_axis_option(config["tick_size"], "tick_size", nonnegative=True))
    if "tip_length" in config:
        tip_length = _finite_axis_option(config["tip_length"], "tip_length")
        if tip_length <= 0:
            raise ValueError("tip_length must be positive")
        engine_call(options.setAxisTipLength, index, tip_length)
    if "color" in config:
        color = _compat._as_color("axis color", config["color"])
        engine_call(options.setAxisColor, index, color.red, color.green, color.blue, color.alpha)
    for key, method in (("stroke_width", "setAxisStrokeWidth"),
                        ("stroke_opacity", "setAxisStrokeOpacity"),
                        ("opacity", "setAxisOpacity")):
        if key in config:
            value = (_compat._manim_stroke_width(config[key]) if key == "stroke_width"
                     else _finite_axis_option(config[key], key))
            engine_call(getattr(options, method), index, value)


def _three_d_axis_label_object(label):
    if isinstance(label, str):
        from _manim_latex import MathTex

        label = MathTex(label)
    if isinstance(label, (_compat.Group, _compat.VGroup)):
        return label
    if isinstance(label, _base.Mobject):
        return _compat.Group(label)
    raise TypeError("ThreeDAxes labels require strings or retained text objects")


def _axes_label_object(label):
    if isinstance(label, bool):
        raise TypeError("Axes labels require strings, numbers, or retained text objects")
    if isinstance(label, (int, float)):
        label = str(label)
    if isinstance(label, str):
        return _three_d_axis_label_object(label)
    if isinstance(label, (_base.Mobject, _compat.Group)):
        if getattr(label, "_semantic_family_handle", None) is None and getattr(
            label, "_semantic_handle", None
        ) is None:
            raise TypeError("Axes labels require a retained semantic text object")
        return label
    raise TypeError("Axes labels require strings, numbers, or retained text objects")


class NumberLine(_compat.Group):
    """Linear or LogBase number line with shared Rust queries and retained ticks."""
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke


    def __init__(self, x_range=None, *, length=None, unit_size=1.0, rotation=0.0,
                 scaling=None,
                 include_ticks=True, tick_size=0.1, exclude_origin_tick=False,
                 include_tip=False, numbers_with_elongated_ticks=None,
                 longer_tick_multiple=2, include_numbers=False,
                 numbers_to_exclude=None, numbers_to_include=None,
                 font_size=36, label_direction=None,
                 line_to_number_buff=None, tip_width=None, tip_height=None,
                 tip_shape=None, label_constructor=None,
                 decimal_number_config=None,
                 color=None, **kwargs):
        values = () if x_range is None else tuple(x_range)
        if x_range is not None and len(values) not in (2, 3):
            raise ValueError("NumberLine x_range requires two or three values")
        scale_kind, scale_parameter, custom_scale_labels = _number_line_scale(scaling)
        if custom_scale_labels and (include_numbers or numbers_to_include is not None):
            raise NotImplementedError(_UNSUPPORTED_LOG_LABELS)
        tick_size = _finite_axis_option(tick_size, "tick_size", nonnegative=True)
        font_size, label_direction, line_to_number_buff, decimal_places = _number_label_style(
            font_size, _base.DOWN if label_direction is None else label_direction,
            _base.MED_SMALL_BUFF if line_to_number_buff is None else line_to_number_buff,
            decimal_number_config, "NumberLine",
        )
        if tip_width is None:
            tip_width = _DEFAULT_NUMBER_LINE_TIP_SIZE
        if tip_height is None:
            tip_height = _DEFAULT_NUMBER_LINE_TIP_SIZE
        tip_width = _finite_axis_option(tip_width, "tip_width")
        tip_height = _finite_axis_option(tip_height, "tip_height")
        if tip_width <= 0 or tip_height <= 0:
            raise ValueError("tip_width and tip_height must be positive")
        if tip_shape is not None:
            raise NotImplementedError("custom NumberLine tip shapes are not supported")
        if label_constructor is not None:
            raise NotImplementedError(
                "NumberLine constructor label_constructor requires the TeX label profile"
            )
        decimal_number_config = _decimal_number_options(
            decimal_number_config, "NumberLine.decimal_number_config",
        )
        numbers_to_include = (
            None if numbers_to_include is None else list(numbers_to_include)
        )
        numbers_to_exclude = (
            [] if numbers_to_exclude is None else list(numbers_to_exclude)
        )
        context = _coordinate_constructor_context()
        options = engine_call(
            _coordinate_options.numberLine, _array(values),
            None if length is None else float(length), float(unit_size), float(rotation),
        )
        try:
            engine_call(options.setTicks, bool(include_ticks), tick_size, bool(exclude_origin_tick))
            engine_call(options.setTips, bool(include_tip))
            engine_call(options.setNumberLineTipDimensions, tip_width, tip_height)
            engine_call(options.setNumberLineScale, scale_kind, scale_parameter)
            engine_call(options.setElongatedTicks,
                        _array(() if numbers_with_elongated_ticks is None else numbers_with_elongated_ticks),
                        float(longer_tick_multiple))
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        _attach_number_line(
            self,
            engine_call(context.liveCreateCoordinates if context is not None else _create_coordinates, options),
            tick_size=tick_size,
            include_ticks=bool(include_ticks),
            include_tip=bool(include_tip),
            exclude_origin_tick=bool(exclude_origin_tick),
        )
        self.tip_width = tip_width
        self.tip_height = tip_height
        self.scaling = scaling if scaling is not None else LinearBase()
        self.include_numbers = bool(include_numbers)
        self.numbers_to_include = numbers_to_include
        self.numbers_to_exclude = numbers_to_exclude
        self.font_size = font_size
        self.label_direction = label_direction
        self.line_to_number_buff = line_to_number_buff
        if decimal_places is None:
            # Manim derives precision before coercing the range to floats.
            step = values[2] if len(values) == 3 else 1
            decimal_places = _decimal_places_from_step(step)
        decimal_number_config.setdefault("num_decimal_places", decimal_places)
        self.decimal_number_config = decimal_number_config
        self.number_label_decimal_places = decimal_places
        if self.include_numbers or numbers_to_include is not None:
            self.add_numbers(
                x_values=numbers_to_include,
                excluding=numbers_to_exclude,
            )

    @property
    def shaft(self):
        return self.submobjects[0]

    @property
    def ticks(self):
        return self.submobjects[1]

    @property
    def tip(self):
        return self._tip

    @tip.setter
    def tip(self, value):
        self._tip = value

    def _coordinate_frame(self):
        context = _coordinate_context([self.shaft])
        if context is not None:
            return engine_call(context.queryNumberLineFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.numberLineFrame)

    @property
    def x_range(self):
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(frame.range))

    def number_to_point(self, number):
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.numberToPoint, float(number)))

    n2p = number_to_point
    __matmul__ = number_to_point

    def point_to_number(self, point):
        point = _base._as_vec2(point)
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.pointToNumber, point.x, point.y))

    p2n = point_to_number

    def __rmatmul__(self, point):
        if isinstance(point, _base.Mobject):
            point = point.get_center()
        return self.point_to_number(point)

    def get_unit_size(self):
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.unitSize))

    def get_start(self):
        return self.get_start_and_end()[0]

    def get_end(self):
        return self.get_start_and_end()[1]

    def get_start_and_end(self):
        with _owned(self._coordinate_frame()) as frame:
            start = engine_call(frame.start)
            end = engine_call(frame.end)
        return _base.Vec2(*start), _base.Vec2(*end)

    def get_vector(self):
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.vector))

    def get_length(self):
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.length))

    def get_angle(self):
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.angle))

    def get_slope(self):
        with _owned(self._coordinate_frame()) as frame:
            return float(engine_call(frame.slope))

    def get_projection(self, point):
        point = _base._as_vec2(point)
        with _owned(self._coordinate_frame()) as frame:
            projected = engine_call(frame.projectPoint, point.x, point.y)
        return _base.Vec2(*projected)

    def set_angle(self, angle, about_point=None):
        angle = _base._ir._finite_number("angle", angle)
        if about_point is None:
            about_point = self.get_start()
        return self.rotate(angle - self.get_angle(), about_point=about_point)

    def set_length(self, length):
        length = _base._ir._finite_number("length", length)
        return self.scale(length / self.get_length())

    def rotate_about_zero(self, angle, axis=_base.OUT, **kwargs):
        return self.rotate(
            angle,
            axis=axis,
            about_point=self.number_to_point(0),
            **kwargs,
        )

    def rotate_about_number(self, number, angle, axis=_base.OUT, **kwargs):
        return self.rotate(
            angle,
            axis=axis,
            about_point=self.number_to_point(number),
            **kwargs,
        )

    def get_tick_range(self):
        """Return Manim's linear tick values through the shared Rust query."""
        import numpy as np
        with _owned(self._coordinate_frame()) as frame:
            values = engine_call(
                frame.tickValues,
                bool(self.include_tip),
                bool(self.exclude_origin_tick),
            )
            return np.asarray(tuple(float(value) for value in values), dtype=float)

    def get_tick(self, x, size=None):
        """Create one Manim-style tick using the current line orientation."""
        x = _base._ir._finite_number("x", x)
        size = self.tick_size if size is None else _base._ir._finite_number("size", size)
        with _owned(self._coordinate_frame()) as frame:
            endpoints = engine_call(frame.tickSegment, x, size)
        tick = _compat.Line(_base.Vec2(float(endpoints[0]), float(endpoints[1])),
                            _base.Vec2(float(endpoints[2]), float(endpoints[3])))
        tick.match_style(self.shaft)
        return tick

    def get_tick_marks(self):
        return self.ticks

    def get_number_mobject(self, number, **kwargs):
        """Return one detached DecimalNumber label at a numeric position.

        Pass ``font=...`` to select the explicit native-Text subset.
        """
        from _manim_number_labels import number_mobjects
        number = _base._ir._finite_number("number", number)
        config = self._number_label_config(kwargs)
        config.setdefault("exclude_zero", False)
        labels = number_mobjects(self, (number,), attach=False, config=config)
        members = labels.submobjects
        if len(members) != 1:
            raise ValueError("number-label options excluded the requested number")
        return members[0]

    def get_number_mobjects(self, *numbers, **kwargs):
        """Return detached labels; no arguments selects automatic tick values."""
        from _manim_number_labels import number_mobjects
        config = self._number_label_config(kwargs)
        config.setdefault("exclude_zero", False)
        return number_mobjects(self, numbers or None, attach=False, config=config)

    def _number_label_config(self, values):
        from _manim_number_labels import _merge_label_config
        config = _merge_label_config(getattr(self, "decimal_number_config", {}), values)
        config.setdefault("direction", getattr(self, "label_direction", _base.DOWN))
        config.setdefault(
            "buff", getattr(self, "line_to_number_buff", _base.MED_SMALL_BUFF)
        )
        config.setdefault("font_size", getattr(self, "font_size", 36))
        if "decimal_places" not in config:
            config["decimal_places"] = getattr(self, "number_label_decimal_places", 0)
        return config

    def add_labels(self, dict_values, direction=None, buff=None, font_size=None,
                   label_constructor=None):
        """Attach caller strings/numbers as positioned native Text labels.

        Existing Mobject labels and TeX/custom constructors remain unsupported
        by this native-Text profile.
        """
        if label_constructor is not None:
            raise NotImplementedError(
                "NumberLine.add_labels custom constructors require the TeX label profile"
            )
        from _manim_number_labels import custom_labels
        config = self._number_label_config({})
        if direction is not None:
            config["direction"] = direction
        if buff is not None:
            config["buff"] = buff
        if font_size is not None:
            config["font_size"] = font_size
        custom_labels(self, dict_values, config=config)
        return self

    def add_numbers(self, x_values=None, excluding=None, **kwargs):
        """Atomically append DecimalNumber labels before playback; expose .numbers.

        Repeated calls append another label family, rather than replacing the
        earlier one. Call ``await prepare_latex()`` first. Passing ``font`` opts
        into the native-Text subset. Explicit values keep ordinary numeric labels
        even when the line has logarithmic scaling.
        """
        from _manim_number_labels import number_mobjects
        config = self._number_label_config(kwargs)
        config.setdefault("exclude_zero", False)
        if excluding is None:
            excluding = getattr(self, "numbers_to_exclude", ())
        number_mobjects(self, x_values, attach=True, config=config, excluding=excluding)
        return self

    def label_plan(self, numbers=None, *, decimal_places=0, exclude_zero=True):
        """Noon extension: immutable text/anchor preparation, not label objects.

        None selects Rust's tick values; an empty iterable selects no labels.
        Place ordinary Text objects with next_to(label.point, ...) and explicitly
        group them with the axes when they should move together. Re-prepare after
        moving the axes; this snapshot is not a live coordinate handle.
        """
        if not isinstance(decimal_places, int) or isinstance(decimal_places, bool):
            raise TypeError("decimal_places must be an integer")
        with _owned(self._coordinate_frame()) as frame:
            with _owned(engine_call(
                frame.numberLabelPlan, _array(() if numbers is None else numbers),
                numbers is None, decimal_places, bool(exclude_zero),
            )) as plan:
                values = tuple(float(x) for x in engine_call(plan.numbers))
                texts = tuple(str(x) for x in engine_call(plan.texts))
                points = _point_pairs(engine_call(plan.points))
                return tuple(_NumberLabel(*entry) for entry in zip(values, texts, points, strict=True))


class UnitInterval(NumberLine):
    """Unit interval with shared Rust endpoint ticks and coordinate queries.

    Numeric label formatting remains the explicit NumberLine native-Text API.
    """

    def __init__(self, unit_size=10, numbers_with_elongated_ticks=None, **kwargs):
        super().__init__((0, 1, 0.1), unit_size=unit_size,
                         numbers_with_elongated_ticks=(0, 1) if numbers_with_elongated_ticks is None
                         else numbers_with_elongated_ticks, **kwargs)


class Axes(_compat.Group):
    """2D axes with Manim v0.21 ranges, dimensions, filled tips, and scales."""
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke


    def __init__(self, x_range=None, y_range=None, *, x_length=None, y_length=None, tips=True,
                 include_ticks=True, tick_size=0.1, axis_config=None,
                 x_axis_config=None, y_axis_config=None, color=None, **kwargs):
        x_options, x_scale, y_options, y_scale = _axes_scalings(
            axis_config, x_axis_config, y_axis_config
        )
        x_range_values = () if x_range is None else tuple(x_range)
        y_range_values = () if y_range is None else tuple(y_range)
        tick_size = _finite_axis_option(tick_size, "tick_size", nonnegative=True)
        context = _coordinate_constructor_context()
        options = engine_call(
            _coordinate_options.axes,
            _array(x_range_values),
            _array(y_range_values),
            None if x_length is None else float(x_length),
            None if y_length is None else float(y_length),
        )
        try:
            engine_call(options.setTicks, bool(include_ticks), tick_size, True)
            engine_call(options.setTips, bool(tips))
            engine_call(options.setAxesScale, 0, x_scale[0], x_scale[1])
            engine_call(options.setAxesScale, 1, y_scale[0], y_scale[1])
            _coordinate_style(options, color, kwargs)
        except BaseException:
            options.free()
            raise
        handle = engine_call(context.liveCreateCoordinates if context is not None else _create_coordinates, options)
        members = []
        axis_setups = (
            (0, x_options, x_scale, x_range_values),
            (1, y_options, y_scale, y_range_values),
        )
        for index, config, scale, _ in axis_setups:
            exclude_origin = scale[0] != "log"
            axis = _attach_number_line(
                object.__new__(NumberLine),
                engine_call(handle.coordinateAxis, index),
                tick_size=float(tick_size),
                include_ticks=bool(include_ticks),
                include_tip=bool(tips),
                exclude_origin_tick=exclude_origin,
            )
            axis.scaling = config["scaling"] if config["scaling"] is not None else LinearBase()
            members.append(axis)
        _coordinate_family(self, handle, members)
        self.axes = _compat.VGroup(*members)
        for axis, (_, config, _, range_values) in zip(members, axis_setups):
            _configure_axes_numeric_labels(axis, config, range_values)

    def get_axes(self):
        """Return a retained family grouping the two coordinate axes."""
        return self.axes

    def get_axis(self, index):
        """Return one axis by its Manim index (x=0, y=1)."""
        return self.get_axes()[index]

    def get_x_axis(self):
        return self.get_axis(0)

    def get_y_axis(self):
        return self.get_axis(1)

    def get_x_unit_size(self):
        return self.get_x_axis().get_unit_size()

    def get_y_unit_size(self):
        return self.get_y_axis().get_unit_size()

    def _get_axis_label(self, label, axis, edge, direction, buff=_base.SMALL_BUFF):
        edge = _base._as_vec2(edge)
        direction = _base._as_vec2(direction)
        buff = _finite_axis_option(buff, "buff", nonnegative=True)
        label = _axes_label_object(label)
        _shared._next_to_edge_in_frame(label, axis, edge, direction, buff,
                                       _base.MED_SMALL_BUFF)
        return label

    def get_x_axis_label(self, label, edge=_base.UR, direction=_base.UR,
                         buff=_base.SMALL_BUFF, **kwargs):
        """Create a retained label next to the upper-right x-axis edge.

        Strings and numeric values use MathTex; call await prepare_latex() first.
        Retained text mobjects are placed without recompiling.
        """
        if kwargs:
            raise TypeError("unsupported Axes label option(s): " + ", ".join(sorted(kwargs)))
        return self._get_axis_label(label, self.get_x_axis(), edge, direction, buff)

    def get_y_axis_label(self, label, edge=_base.UR,
                         direction=_base.UP * 0.5 + _base.RIGHT,
                         buff=_base.SMALL_BUFF, **kwargs):
        """Create a retained label next to the upper-right y-axis edge."""
        if kwargs:
            raise TypeError("unsupported Axes label option(s): " + ", ".join(sorted(kwargs)))
        return self._get_axis_label(label, self.get_y_axis(), edge, direction, buff)

    def get_axis_labels(self, x_label="x", y_label="y"):
        """Create and retain the default x/y label pair."""
        self.axis_labels = _compat.VGroup(
            self.get_x_axis_label(x_label), self.get_y_axis_label(y_label)
        )
        return self.axis_labels

    def get_line_from_axis_to_point(self, index, point, line_func=None,
                                    line_config=None, color=None, stroke_width=2):
        """Create a retained line from one coordinate axis to a scene point.

        Projection uses the selected NumberLine's current shared Rust frame;
        this wrapper only selects the axis and forwards constructor options.
        """
        if isinstance(index, bool):
            raise TypeError("axis index must be an integer, not a boolean")
        try:
            index = _index(index)
        except TypeError:
            raise TypeError("axis index must be an integer") from None
        if index not in (0, 1):
            raise IndexError("2D Axes axis index must be 0 or 1")
        if line_func is None:
            line_func = _base.DashedLine
        if not callable(line_func):
            raise TypeError("line_func must be callable")
        if line_config is None:
            options = {}
        elif isinstance(line_config, Mapping):
            options = dict(line_config)
        else:
            raise TypeError("line_config must be a mapping")

        point = _base._as_vec2(point)
        axis = self.get_axis(index)
        axis_point = axis.get_projection(point)
        options["color"] = _compat._as_color(
            "line color", _base.WHITE if color is None else color
        )
        options["stroke_width"] = stroke_width
        return line_func(axis_point, point, **options)

    def get_vertical_line(self, point, **kwargs):
        """Create a vertical line from the x-axis to a scene point."""
        return self.get_line_from_axis_to_point(0, point, **kwargs)

    def get_horizontal_line(self, point, **kwargs):
        """Create a horizontal line from the y-axis to a scene point."""
        return self.get_line_from_axis_to_point(1, point, **kwargs)

    def get_lines_to_point(self, point, **kwargs):
        """Create horizontal and vertical guide lines to a scene point."""
        return _compat.VGroup(
            self.get_horizontal_line(point, **kwargs),
            self.get_vertical_line(point, **kwargs),
        )

    @property
    def x_axis(self):
        return self.submobjects[0]

    @property
    def y_axis(self):
        return self.submobjects[1]

    def _coordinate_frame(self):
        # Observe only two shaft identities, not every tick/label in the family.
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            return engine_call(context.queryAxesFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.axesFrame)

    def coords_to_point(self, x, y=0.0):
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.coordsToPoint, float(x), float(y)))

    c2p = coords_to_point

    def point_to_coords(self, point):
        point = _base._as_vec2(point)
        with _owned(self._coordinate_frame()) as frame:
            return _base.Vec2(*engine_call(frame.pointToCoords, point.x, point.y))

    p2c = point_to_coords

    def get_origin(self):
        return self.c2p(0.0, 0.0)

    def plot(self, function, x_range=None, *, use_smoothing=True,
             discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        with _owned(self._coordinate_frame()) as frame:
            plan = engine_call(frame.plotPlan, _array(() if x_range is None else x_range),
                               _array(discontinuities), None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=False)
        graph = _curve(object.__new__(FunctionGraph), options, color, kwargs)
        graph.underlying_function = function
        # Manim's graph callable belongs to the Axes that created the graph.
        # Queries through another Axes still use this owner's current frame;
        # querying does not resample the retained curve.
        graph.function = lambda x: self.c2p(float(x), float(function(float(x))))
        return graph

    def input_to_graph_point(self, x, graph):
        """Return a callable-backed graph point; retained-path inversion is unsupported."""
        _outside_callback()
        function = getattr(graph, "function", None)
        underlying_function = getattr(graph, "underlying_function", None)
        if not callable(function) or not callable(underlying_function):
            raise NotImplementedError(
                "input_to_graph_point currently requires a callable-backed graph; "
                "retained-path fallback is not supported"
            )
        return _base._as_vec2(function(float(x)))

    i2gp = input_to_graph_point

    def plot_samples(self, points, *, color=None, **kwargs):
        """Preserve sample order and repeated x values as a retained polyline."""
        with _owned(self._coordinate_frame()) as frame:
            options = engine_call(frame.sampledPlot, _points(points))
        return _curve(object.__new__(_compat.VMobject), options, color, kwargs)

    def get_area(self, graph, x_range=None, color=None, opacity=0.3,
                 bounded_graph=None, **kwargs):
        """Create one static closed path prepared by the shared Rust query."""
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        if context is not None:
            options = engine_call(
                context.effectiveAxesAreaOptions, self._semantic_family_handle,
                graph._semantic_handle, _array(() if x_range is None else x_range),
                graph._semantic_handle if bounded_graph is None else bounded_graph._semantic_handle,
                bounded_graph is not None,
            )
        else:
            with _owned(self._coordinate_frame()) as frame:
                options = engine_call(
                    frame.area, graph._semantic_handle,
                    _array(() if x_range is None else x_range),
                    graph._semantic_handle if bounded_graph is None else bounded_graph._semantic_handle,
                    bounded_graph is not None,
                )
        kwargs.setdefault("fill_opacity", float(opacity))
        kwargs.setdefault("stroke_opacity", float(opacity))
        colors = (_base.BLUE, _base.GREEN) if color is None else color
        if isinstance(colors, (list, tuple)):
            result = _curve(object.__new__(_compat.VMobject), options, None, kwargs)
            return result.set_color_by_gradient(*colors)
        return _curve(object.__new__(_compat.VMobject), options, colors, kwargs)

    def get_riemann_rectangles(self, graph, x_range=None, dx=0.1,
                               input_sample_type="left", stroke_width=1,
                               stroke_color=_base.BLACK, fill_opacity=1,
                               color=(_base.BLUE, _base.GREEN), show_signed_area=True,
                               bounded_graph=None, blend=False, width_scale_factor=1.001):
        """Sample original scalar callables against one Rust-captured snapshot."""
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        sample = {"left": 0, "right": 1, "center": 2}.get(input_sample_type)
        if sample is None:
            raise ValueError("input_sample_type must be 'left', 'right', or 'center'")
        colors = color if isinstance(color, (tuple, list)) else (color,)
        colors = [_compat._as_color("Riemann color", value) for value in colors]
        stroke_color = _compat._as_color("Riemann stroke color", stroke_color)
        bounded = graph if bounded_graph is None else bounded_graph
        graph_handle, bounded_handle = graph._semantic_handle, bounded._semantic_handle
        if context is None:
            prepare = self._semantic_family_handle.riemannSamplePlan
            prepare_args = (graph_handle,)
        else:
            prepare = context.liveEffectiveRiemannSamplePlan
            prepare_args = (self._semantic_family_handle, graph_handle)
        options = engine_call(_coordinate_options.riemann, _array(() if x_range is None else x_range),
                              float(dx), sample, float(width_scale_factor))
        try:
            engine_call(options.setPaint, _shared._gradient_components(colors),
                        _shared._gradient_components([stroke_color]), float(stroke_width),
                        float(fill_opacity), bool(show_signed_area), bool(blend))
        except BaseException:
            options.free()
            raise
        plan = engine_call(prepare, *prepare_args, options, bounded_handle,
                           bounded_graph is not None)
        with _owned(plan):
            function = getattr(graph, "underlying_function", None)
            bounded_function = (None if bounded_graph is None else
                                getattr(bounded_graph, "underlying_function", None))
            top_values, baseline_values = [], []
            # Preserve per-rectangle top-then-lower invocation order. Each graph
            # independently uses its callable or the captured Rust path fallback.
            for x, sample_x in zip(engine_call(plan.starts), engine_call(plan.samples), strict=True):
                if function is not None:
                    top_values.append(float(function(float(sample_x))))
                if bounded_function is not None:
                    baseline_values.append(float(bounded_function(float(x))))
            values = (_array(top_values), _array(baseline_values))
            handle = (engine_call(plan.publish, *values) if context is None else
                      engine_call(context.livePublishRiemannPlan, plan, *values))
        members = [_leaf(member, _compat.Rectangle) for member in engine_call(handle.directMobjects)]
        return _family(object.__new__(_compat.VGroup), handle, members)

    def add_coordinates(self, *axes_numbers, x_config=None, y_config=None, **kwargs):
        """Add default or positional per-axis coordinate labels.

        Numeric defaults use retained DecimalNumber labels and require
        ``await prepare_latex()``. Passing ``font`` selects native Text.
        Two-axis numeric labels commit together and must use one text type.
        Add mixed text types or custom mappings one axis at a time.
        """
        if "x_values" in kwargs or "y_values" in kwargs:
            raise TypeError("add_coordinates uses positional per-axis values")
        from _manim_number_labels import add_coordinates
        return add_coordinates(self, *axes_numbers, x_config=x_config,
                               y_config=y_config, config=kwargs)

    def time_series_plan(self, samples, *, run_time):
        """Noon extension: map timestamp/value pairs and prepare interval timing.

        Samples must be finite and strictly increasing in time. The immutable
        result is preparation data for ordinary plays, not a clock or updater.
        No interpolation, sorting, or timestamp normalization occurs in Python.
        """
        with _owned(self._coordinate_frame()) as frame:
            with _owned(engine_call(frame.timeSeriesPlan, _points(samples), float(run_time))) as plan:
                return _TimeSeriesPlan(
                    _point_pairs(engine_call(plan.points)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def synchronized_series_plan(self, series, *, time_range, run_time):
        """Noon extension: prepare multiple recordings in one explicit window.

        Every recording must cover time_range. Rust splits at the union of input
        timestamps and linearly interpolates each series there; it never fills
        missing coverage or extrapolates. This immutable, bounded preparation is
        for ordinary plays, not a streaming player or a live coordinate cache.
        """
        with _owned(self._coordinate_frame()) as frame:
            rows = tuple(tuple(_base._as_vec2(point) for point in row) for row in series)
            values = _array(component for row in rows for point in row for component in point)
            with _owned(engine_call(
                frame.synchronizedSeriesPlan, values, _array(len(row) for row in rows),
                _array(time_range), float(run_time),
            )) as plan:
                return _SynchronizedTimeSeriesPlan(
                    tuple(_point_pairs(engine_call(plan.seriesPoints, index))
                          for index in range(int(engine_call(plan.seriesCount)))),
                    tuple(float(x) for x in engine_call(plan.dataTimes)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def gapped_series_plan(self, series, *, break_after, time_range, run_time):
        """Prepare explicitly disconnected recordings on one shared time grid.

        Supply one strictly increasing list of source break-after indices per
        recording. Index i disconnects samples i and i+1; both measured endpoints
        remain known. Missing points AND missing drawable segments are None.
        Never infer a line from two known points: consult series_segments.
        NaN guessing, extrapolation, and hold-last-value filling are not provided.
        """
        with _owned(self._coordinate_frame()) as frame:
            rows = tuple(tuple(_base._as_vec2(point) for point in row) for row in series)
            breaks = tuple(tuple(_break_index(i) for i in row) for row in break_after)
            values = _array(component for row in rows for point in row for component in point)
            with _owned(engine_call(
                frame.gappedSeriesPlan, values, _array(len(row) for row in rows),
                _array(i for row in breaks for i in row), _array(len(row) for row in breaks),
                _array(time_range), float(run_time),
            )) as plan:
                count = int(engine_call(plan.seriesCount))
                return _GappedTimeSeriesPlan(
                    tuple(tuple(None if p is None or p is _jsnull else _base.Vec2(*map(float, p))
                                for p in engine_call(plan.seriesPoints, i)) for i in range(count)),
                    tuple(tuple(None if pair is None or pair is _jsnull else _point_pairs(pair)
                                for pair in engine_call(plan.seriesSegments, i)) for i in range(count)),
                    tuple(float(x) for x in engine_call(plan.dataTimes)),
                    _point_pairs(engine_call(plan.cursorPoints)),
                    tuple(float(x) for x in engine_call(plan.keyTimes)),
                    tuple(float(x) for x in engine_call(plan.durations)),
                    float(engine_call(plan.runTime)),
                )


    def plot_implicit_curve(self, func, min_depth=5, max_quads=1500, **kwargs):
        from _manim_implicit import plot_implicit_curve
        return plot_implicit_curve(self, func, min_depth, max_quads, **kwargs)


def _attach_three_d_axis(wrapper, handle, tip_handle, *, tick_size=0.1,
                         include_tip=True, exclude_origin_tick=True):
    shaft = _leaf(engine_call(handle.coordinateShaft))
    ticks = _family(
        object.__new__(_compat.VGroup),
        engine_call(handle.coordinateTicks),
        [_leaf(tick) for tick in engine_call(handle.coordinateTickObjects)],
    )
    members = [shaft, ticks]
    wrapper.tip = None
    wrapper.tick_size = tick_size
    wrapper.include_tip = include_tip
    wrapper.exclude_origin_tick = exclude_origin_tick
    if tip_handle is not None:
        wrapper.tip = _leaf(tip_handle, _compat.VMobject)
        members.append(wrapper.tip)
    return _coordinate_family(wrapper, handle, members)


class ThreeDAxes(_compat.Group):
    """Linear X/Y/Z axes with pinned Manim ranges, lengths and positive tips.

    The numeric frame, world-axis transforms, ticks, tips, Cairo axis pieces,
    shading, and coordinate conversions are Rust-owned. Axis labels use
    retained Text or MathTex families. The axis config accepts bounded stroke,
    tick, and filled-tip options.
    """
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke


    def __init__(self, x_range=(-6, 6, 1), y_range=(-5, 5, 1),
                 z_range=(-4, 4, 1), x_length=10.5, y_length=10.5,
                 z_length=6.5, *, tips=True, include_ticks=True, tick_size=0.1,
                 axis_config=None, x_axis_config=None, y_axis_config=None, z_axis_config=None,
                 labels=None, color=None, num_axis_pieces=20,
                 light_source=(-7, -9, 10), shade_in_3d=True, **kwargs):
        context = _coordinate_constructor_context()
        if labels is not None:
            raise NotImplementedError("ThreeDAxes axis labels are not yet supported")
        if kwargs:
            raise TypeError("unsupported ThreeDAxes option(s): " + ", ".join(sorted(kwargs)))
        if isinstance(num_axis_pieces, bool):
            raise TypeError("num_axis_pieces requires an integer")
        try:
            num_axis_pieces = _index(num_axis_pieces)
        except TypeError as error:
            raise TypeError("num_axis_pieces requires an integer") from error
        if not 1 <= num_axis_pieces <= 256:
            raise ValueError("num_axis_pieces must be between 1 and 256")
        if not isinstance(shade_in_3d, bool):
            raise TypeError("shade_in_3d requires a boolean")
        try:
            light_source = tuple(light_source)
        except TypeError as error:
            raise TypeError("light_source requires three finite numbers") from error
        if len(light_source) != 3:
            raise ValueError("light_source requires exactly three values")
        light_source = tuple(
            _finite_axis_option(component, "light_source") for component in light_source
        )
        if max(abs(component) for component in light_source) == 0.0:
            raise ValueError("light_source must be nonzero")
        common = _three_d_axis_config(axis_config, "axis_config")
        axis_configs = [
            _three_d_axis_config(value, name)
            for value, name in ((x_axis_config, "x_axis_config"),
                                (y_axis_config, "y_axis_config"),
                                (z_axis_config, "z_axis_config"))
        ]
        global_tips = common.pop("include_tip", tips)
        global_ticks = common.pop("include_ticks", include_ticks)
        global_tick_size = common.pop("tick_size", tick_size)
        global_exclude_origin = common.pop("exclude_origin_tick", True)
        global_tip_length = common.pop("tip_length", None)
        if not all(isinstance(value, bool) for value in (global_tips, global_ticks, global_exclude_origin)):
            raise TypeError("ThreeDAxes tips, include_ticks, and exclude_origin_tick require booleans")
        if isinstance(global_tick_size, bool):
            raise TypeError("ThreeDAxes tick_size requires a finite number")
        global_tick_size = float(global_tick_size)
        if not math.isfinite(global_tick_size) or global_tick_size < 0:
            raise ValueError("ThreeDAxes tick_size must be finite and nonnegative")
        options = engine_call(
            _coordinate_options.threeDAxes, _array(x_range), _array(y_range),
            _array(z_range), float(x_length), float(y_length), float(z_length),
        )
        try:
            engine_call(options.setTicks, global_ticks, global_tick_size, global_exclude_origin)
            engine_call(options.setTips, global_tips)
            engine_call(options.setNumAxisPieces, num_axis_pieces)
            engine_call(options.setAxisLightDirection, *light_source)
            engine_call(options.setShadeIn3D, shade_in_3d)
            if global_tip_length is not None:
                global_tip_length = _finite_axis_option(global_tip_length, "tip_length")
                if global_tip_length <= 0:
                    raise ValueError("tip_length must be positive")
                engine_call(options.setTipLength, global_tip_length)
            _coordinate_style(options, common.pop("color", color), common)
            for index, config in enumerate(axis_configs):
                _apply_three_d_axis_config(options, index, config)
        except BaseException:
            options.free()
            raise
        handle = engine_call(
            context.liveCreateCoordinates if context is not None else _create_coordinates,
            options,
        )
        axes = [
            _attach_three_d_axis(
                object.__new__(NumberLine),
                engine_call(handle.threeDAxesAxis, index),
                engine_call(handle.threeDAxesTip, index),
                tick_size=_finite_axis_option(
                    axis_configs[index].get("tick_size", global_tick_size),
                    "tick_size",
                    nonnegative=True,
                ),
                include_tip=axis_configs[index].get("include_tip", global_tips),
                exclude_origin_tick=axis_configs[index].get(
                    "exclude_origin_tick", global_exclude_origin
                ),
            )
            for index in range(3)
        ]
        _family(self, handle, axes)

    def _axis_label_family(self, labels, *, buff=0.1, fixed_orientation=False, **kwargs):
        if kwargs:
            raise NotImplementedError("unsupported ThreeDAxes label option(s): " + ", ".join(sorted(kwargs)))
        if not isinstance(fixed_orientation, bool):
            raise TypeError("fixed_orientation requires a boolean")
        buff = _finite_axis_option(buff, "buff", nonnegative=True)
        selected = []
        handles = [None, None, None]
        for index, label in labels:
            label = _three_d_axis_label_object(label)
            handle = getattr(label, "_semantic_family_handle", None)
            if handle is None:
                raise TypeError("ThreeDAxes labels require fresh retained semantic family handles")
            selected.append(label)
            handles[index] = handle
        if not selected:
            raise ValueError("at least one ThreeDAxes label is required")
        selected_axes = sum(1 << index for index, handle in enumerate(handles) if handle is not None)
        active_handle = next(handle for handle in handles if handle is not None)
        family = engine_call(
            self._semantic_family_handle.threeDAxesLabelFamilies,
            *(active_handle if handle is None else handle for handle in handles),
            selected_axes, buff, fixed_orientation,
        )
        wrapper = object.__new__(_compat.Group)
        return _family(wrapper, family, selected)

    def get_x_axis_label(self, label, *, buff=0.1, fixed_orientation=False, **kwargs):
        """Place one existing retained text object at the positive X endpoint."""
        return self._axis_label_family(((0, label),), buff=buff,
                                       fixed_orientation=fixed_orientation, **kwargs)

    def get_y_axis_label(self, label, *, buff=0.1, fixed_orientation=False, **kwargs):
        """Place one existing retained text object at the positive Y endpoint."""
        return self._axis_label_family(((1, label),), buff=buff,
                                       fixed_orientation=fixed_orientation, **kwargs)

    def get_z_axis_label(self, label, *, buff=0.1, fixed_orientation=False, **kwargs):
        """Place one existing retained text object at the positive Z endpoint."""
        return self._axis_label_family(((2, label),), buff=buff,
                                       fixed_orientation=fixed_orientation, **kwargs)

    def get_axis_labels(self, x_label="x", y_label="y", z_label="z", *, buff=0.1,
                        fixed_orientation=False, **kwargs):
        """Place three existing retained text objects in one Rust transaction.

        Strings use the existing retained MathTex compiler path, which must
        already be prepared. Existing Text and MathTex families retain their
        Rust-owned layout and screen-edge correction.
        """
        return self._axis_label_family(((0, x_label), (1, y_label), (2, z_label)),
                                       buff=buff, fixed_orientation=fixed_orientation,
                                       **kwargs)

    @property
    def x_axis(self):
        return self.submobjects[0]

    @property
    def y_axis(self):
        return self.submobjects[1]

    @property
    def z_axis(self):
        return self.submobjects[2]

    def _coordinate_frame(self):
        shafts = [self.x_axis.shaft, self.y_axis.shaft, self.z_axis.shaft]
        context = _coordinate_context(shafts)
        if context is not None:
            return engine_call(context.queryThreeDAxesFrame, self._semantic_family_handle)
        return engine_call(self._semantic_family_handle.threeDAxesFrame)

    def coords_to_point(self, x, y, z):
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(
                frame.coordsToPoint, float(x), float(y), float(z)))

    c2p = coords_to_point

    def point_to_coords(self, point):
        try:
            values = tuple(float(component) for component in point)
        except TypeError as error:
            raise TypeError("point must contain three numeric coordinates") from error
        if len(values) != 3:
            raise ValueError("point must contain three numeric coordinates")
        with _owned(self._coordinate_frame()) as frame:
            return tuple(float(value) for value in engine_call(
                frame.pointToCoords, *values))

    p2c = point_to_coords

class BarChart(Axes):
    """Shared-Rust static bar chart with explicit, atomic value changes.

    The compatibility wrapper owns argument coercion and wrapper identity. Axes,
    rectangle layout, color gradients and updates stay in the typed Rust chart.
    """

    def __init__(self, values, bar_names=None, y_range=None, x_length=None,
                 y_length=None, bar_colors=None, bar_width=0.6,
                 bar_fill_opacity=0.7, bar_stroke_width=3, **kwargs):
        x_config = dict(kwargs.pop("x_axis_config", {}))
        name_size = float(x_config.pop("font_size", 24))
        y_config = dict(kwargs.pop("y_axis_config", {}))
        label_settings = {"font": "DejaVu Sans Mono", "font_size": 36, "buff": 0.25, "direction": _base.LEFT}
        aliases = {"label_direction": "direction", "line_to_number_buff": "buff"}
        for key, value in y_config.items():
            label_settings[aliases.get(key, key)] = value
        if kwargs or x_config:
            unsupported = sorted(kwargs) + ["x_axis_config." + key for key in sorted(x_config)]
            raise NotImplementedError("unsupported BarChart option(s): " + ", ".join(unsupported))
        if bar_names is not None:
            bar_names = tuple(bar_names)
            if not all(isinstance(name, str) for name in bar_names):
                raise TypeError("bar names must be strings")
        values = tuple(float(value) for value in values)
        if _bar_chart_options is None:
            raise RuntimeError("BarChart requires the shared Rust authoring host")
        options = engine_call(_bar_chart_options, _array(values),
                              _array(() if y_range is None else y_range),
                              0.0 if x_length is None else float(x_length),
                              0.0 if y_length is None else float(y_length))
        try:
            engine_call(options.setStyle, float(bar_width), float(bar_fill_opacity), float(bar_stroke_width))
            if bar_colors is not None:
                colors = bar_colors if isinstance(bar_colors, (tuple, list)) else (bar_colors,)
                rgba = [component for color in colors for component in (
                    _compat._as_color("bar color", color).red,
                    _compat._as_color("bar color", color).green,
                    _compat._as_color("bar color", color).blue,
                    _compat._as_color("bar color", color).alpha,
                )]
                engine_call(options.setColors, _array(rgba))
            context = _shared._live_constructor_context("bar chart", allow_unstarted=True)
            if bar_names is not None:
                engine_call(options.setNames, _to_js(list(bar_names)), name_size)
            from _manim_number_labels import _options
            label_options, size, color = _options(label_settings)
        except BaseException:
            options.free()
            raise
        self._bar_chart_handle = engine_call(_create_bar_chart, options, label_options, context)
        bars = engine_call(self._bar_chart_handle.bars)
        self.bars = _family(object.__new__(_compat.VGroup), bars,
                            [_leaf(handle, _compat.Rectangle) for handle in engine_call(bars.directMobjects)])
        axes = engine_call(self._bar_chart_handle.axes)
        axis_members = [
            _attach_number_line(object.__new__(NumberLine), engine_call(axes.coordinateAxis, index))
            for index in (0, 1)
        ]
        self.axes = _family(object.__new__(Axes), axes, axis_members)
        from _manim_number_labels import _family as _numeric_family, _remember
        names = engine_call(self._bar_chart_handle.xLabels)
        if names is not None and names is not _jsnull:
            self.x_axis.labels = _chart_text_family(names, name_size, context)
            self.x_axis._semantic_member_wrappers[_shared._family_wrapper_key(self.x_axis.labels)] = self.x_axis.labels
        labels = engine_call(self._bar_chart_handle.yLabels)
        _remember(self.y_axis, _numeric_family(labels, size, color))
        _family(self, engine_call(self._bar_chart_handle.family), [self.bars, self.axes])
        self._bar_chart_context = context

    def _rehydrate_semantic_family_handle(self):
        """Rebind this copied wrapper to its copied Rust chart family."""
        self._bar_chart_handle = engine_call(self._semantic_family_handle.barChart)
        self.__dict__.pop("_bar_chart_context", None)

    @property
    def x_axis(self):
        return self.axes.x_axis

    @property
    def y_axis(self):
        return self.axes.y_axis

    @property
    def values(self):
        return list(engine_call(self._bar_chart_handle.values))

    @property
    def _coordinate_handle(self):
        return self.axes._coordinate_handle

    def get_bar_labels(self, color=None, font_size=24, buff=0.25, label_constructor=None):
        from _manim_latex import Tex, MathTex
        constructor = Tex if label_constructor is None else label_constructor
        if constructor not in (Tex, MathTex):
            raise NotImplementedError("BarChart labels support Tex or MathTex")
        if _bar_labels is None:
            raise RuntimeError("BarChart labels require the shared Rust authoring host")
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        rgba = []
        if color is not None:
            c = _compat._as_color("label color", color)
            rgba = [c.red, c.green, c.blue, c.alpha]
        handle = engine_call(_bar_labels, self._bar_chart_handle, float(font_size), float(buff),
                             constructor is MathTex, _array(rgba), context)
        return _chart_text_family(handle, float(font_size), context)

    def change_bar_values(self, values, update_colors=True):
        values = tuple(float(value) for value in values)
        context = _coordinate_context([self.x_axis.shaft, self.y_axis.shaft])
        old = engine_call(self._bar_chart_handle.barPrefix, len(values))
        if context is None:
            engine_call(self._bar_chart_handle.changeBarValues, _array(values), bool(update_colors))
        else:
            engine_call(context.liveChangeBarValues, self._bar_chart_handle, _array(values), bool(update_colors))
        new = engine_call(self._bar_chart_handle.barPrefix, len(values))
        changed = []
        for before, after in zip(old, new):
            before_key = f"{int(before.semanticSlot)}:{int(before.semanticGeneration)}"
            after_key = f"{int(after.semanticSlot)}:{int(after.semanticGeneration)}"
            if before_key != after_key:
                previous = self.bars._semantic_member_wrappers.pop(before_key, None)
                replacement = _leaf(after, _compat.Rectangle)
                if context is not None:
                    replacement._canonical_live_target_context = context
                self.bars._semantic_member_wrappers[after_key] = replacement
                changed.extend((previous, replacement) if previous is not None else (replacement,))
        scene = self.x_axis.shaft._scene
        if changed and scene is not None:
            from _manim_scene import _reconcile_completed_family_bindings
            _reconcile_completed_family_bindings(scene, tuple(changed))
        return self


def _chart_text_family(handle, font_size, context=None):
    from _manim_latex import _CompiledTexLeaf

    members = []
    for object_handle, source in engine_call(handle.numberLabelMembers):
        label = object.__new__(_CompiledTexLeaf)
        label._initialize_text(str(source), font_size, object_handle, _base.WHITE, 1.0,
                               presentation_applied=True)
        if context is not None:
            label._canonical_live_target_context = context
        members.append(label)
    return _family(object.__new__(_compat.VGroup), handle, members)


def _callable(function):
    if not callable(function):
        raise TypeError("plot function must be callable")
    _outside_callback()


def _points(points):
    return _array(component for point in points for component in _base._as_vec2(point))


def _evaluate(plan, function, use_smoothing, *, parametric):
    with _owned(plan):
        parameters = engine_call(plan.parameters)
        # Construction evaluates once. Scalar callable identity may be retained
        # for explicit Riemann queries, never for deterministic frame playback.
        if parametric:
            values = _points(function(float(t)) for t in parameters)
            return engine_call(plan.parametricSamples, values, bool(use_smoothing))
        values = _array(function(float(x)) for x in parameters)
        return engine_call(plan.functionSamples, values, bool(use_smoothing))


def _curve(wrapper, options, color, kwargs):
    try:
        _shared._apply_shared_constructor_options(options, kwargs)
        _shared._apply_constructor_color(options, color)
    except BaseException:
        options.free()
        raise
    _shared._attach_geometry_options(wrapper, options, "static plot")
    return wrapper


class ParametricFunction(_compat.VMobject):
    """Preparation-only 2D parametric curve; supply an explicit t range."""

    def __init__(self, function, t_range, *, use_smoothing=True,
                 discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        plan = engine_call(_sampling_plan.parametric, _array(t_range), _array(discontinuities),
                           None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=True)
        _curve(self, options, color, kwargs)


class FunctionGraph(_compat.VMobject):
    """Preparation-only y=f(x) in scene coordinates; supply an explicit range."""

    def __init__(self, function, x_range, *, use_smoothing=True,
                 discontinuities=(), dt=None, color=None, **kwargs):
        _callable(function)
        plan = engine_call(_sampling_plan.parametric, _array(x_range), _array(discontinuities),
                           None if dt is None else float(dt), None)
        options = _evaluate(plan, function, use_smoothing, parametric=False)
        _curve(self, options, color, kwargs)
        self.underlying_function = function
        self.function = lambda x: _base.Vec2(float(x), float(function(float(x))))

    def _function_plot_range(self):
        return tuple(float(value) for value in
                     engine_call(self._semantic_handle.functionPlotRange))

    @property
    def t_min(self):
        return self._function_plot_range()[0]

    @property
    def t_max(self):
        return self._function_plot_range()[1]


__all__ = ["NumberLine", "Axes", "BarChart", "FunctionGraph", "ParametricFunction"]
